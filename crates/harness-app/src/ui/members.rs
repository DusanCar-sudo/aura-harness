//! Members: an orchestrator and its analysts, working one task in three phases.
//!
//! Plan, dispatch, review. The lead divides the task and hands each analyst a
//! job; the analysts run those jobs in parallel; then the lead scores them and
//! closes the task with a verdict. Scores are written to `members.json` in the
//! config dir, so a stray analyst is told about it on its next run.
//!
//! Every member is an ordinary agent loop (`harness_core::agent::run`) over the
//! composer's model, key, tools and MCP servers, so nothing here needs its own
//! provider plumbing. The whole crew runs with full tool access — see
//! `run_swarm` in `main.rs`.

use crate::theme;
use crate::ui::{text_edit, TextEditSpec};
use egui::{RichText, Ui};
use harness_core::agent::{truncate_for_display, LoopEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::sync::mpsc::{error::TryRecvError, Receiver};

/// The crew in row order: the orchestrator first, then the analysts it
/// dispatches. Roles are a code change on purpose — a swarm whose members you
/// can rename and reorder is a config screen nobody asked for.
pub const CREW: [(&str, &str); 4] = [
    (
        "lead",
        "You are the orchestrator of a small analyst crew. You divide the work and \
         judge the results; you do not answer the task yourself.",
    ),
    (
        "research-analyst",
        "You are a research analyst. Establish the facts: what already exists, what \
         the documentation says, and what is already solved. Be specific.",
    ),
    (
        "risk-analyst",
        "You are a risk analyst. Name what can go wrong: failure modes, security \
         holes, data loss, edge cases, and the cost of being wrong.",
    ),
    (
        "critic-analyst",
        "You are a critic. Attack the obvious answer: what the task is missing, what \
         it assumes that may be false, and what the simpler alternative is.",
    ),
];

/// Every member after the orchestrator. Member index `i` in `Swarm::members` is
/// `ANALYSTS[i - 1]`. Const range-indexing is not stable, so the elements are
/// named; the crew-shape test fails if `CREW` grows without this following it.
pub const ANALYSTS: &[(&str, &str)] = &[CREW[1], CREW[2], CREW[3]];

/// What the orchestrator may award an analyst, and what its output is clamped to.
pub const SCORE_MIN: i8 = -2;
pub const SCORE_MAX: i8 = 2;

/// Where the crew is in its three-phase run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// The orchestrator is dividing the task.
    Planning,
    /// The analysts are working on the jobs they were given.
    Dispatched,
    /// The orchestrator is scoring them and closing the task.
    Reviewing,
    Done,
    /// The orchestration vanished mid-run.
    Error,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Planning => "orchestrator is dividing the task",
            Self::Dispatched => "analysts running",
            Self::Reviewing => "orchestrator is scoring",
            Self::Done => "done",
            Self::Error => "stopped",
        }
    }
}

/// What one member is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemberStatus {
    #[default]
    Idle,
    Running,
    Done,
    Error,
}

impl MemberStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Waiting",
            Self::Running => "Running",
            Self::Done => "Finished",
            Self::Error => "Error",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Idle => theme::dim(),
            Self::Running => theme::accent(),
            Self::Done => theme::ok(),
            Self::Error => theme::danger(),
        }
    }
}

/// One agent in the crew: its role, what it was told to do, and what it produced.
pub struct Member {
    pub name: &'static str,
    /// Role brief, appended to the system prompt for this member only.
    pub brief: &'static str,
    pub status: MemberStatus,
    pub answer: String,
    pub tools: usize,
    /// One-line summary of the most recent tool call, shown on the row.
    pub last_tool: String,
    pub error: Option<String>,
    /// The job the orchestrator handed this member. Empty for the orchestrator.
    pub assignment: String,
    /// This run's score and the orchestrator's reason for it.
    pub score: Option<i8>,
    pub reason: String,
}

impl Member {
    fn new(name: &'static str, brief: &'static str) -> Self {
        Self {
            name,
            brief,
            status: MemberStatus::Idle,
            answer: String::new(),
            tools: 0,
            last_tool: String::new(),
            error: None,
            assignment: String::new(),
            score: None,
            reason: String::new(),
        }
    }

    /// True while this member still has work in flight.
    pub fn running(&self) -> bool {
        self.status == MemberStatus::Running
    }

    fn apply(&mut self, event: LoopEvent) {
        match event {
            LoopEvent::Content(text) => self.answer.push_str(&text),
            LoopEvent::Reasoning(_) => {}
            LoopEvent::ToolRequested { summary, .. } => {
                self.tools += 1;
                self.last_tool = summary;
            }
            LoopEvent::ToolFinished {
                output, is_error, ..
            } => {
                // The core caps output at 4000 bytes; a one-line row wants less.
                if is_error {
                    self.last_tool = format!("error: {}", truncate_for_display(&output, 120));
                }
            }
            LoopEvent::TurnComplete(_) => {}
            LoopEvent::Finished { .. } => self.status = MemberStatus::Done,
            LoopEvent::Failed(message) => {
                self.status = MemberStatus::Error;
                self.error = Some(message);
            }
        }
    }
}

/// Everything the crew's one event pipe carries. The index is the member's row.
pub enum CrewEvent {
    Phase(Phase),
    Member(usize, LoopEvent),
    /// The orchestrator gave this analyst its job.
    Assign(usize, String),
    /// The orchestrator scored this analyst.
    Score(usize, i8, String),
}

/// One role's reward record, kept across runs so a score has consequences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    /// Every score this role has earned, added up.
    pub total: i64,
    /// Last run's score, and the orchestrator's reason for it.
    pub last: i8,
    pub reason: String,
}

/// Reward records by role name.
pub type Standings = BTreeMap<String, Standing>;

fn standings_path() -> std::path::PathBuf {
    harness_core::config::config_dir().join("members.json")
}

/// Today's date in UTC, civil calendar.
fn utc_date() -> (i64, u32, u32) {
    let days = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// `YYYYMMDD-HHMMSS` in UTC, for report file names.
fn stamp_utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let clock = secs % 86400;
    let (y, m, d) = utc_date();
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        clock / 3600,
        (clock / 60) % 60,
        clock % 60
    )
}

/// `YYYY-MM-DD HH:MM:SS` in UTC, for report headings.
fn phase_utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let clock = secs % 86400;
    let (y, m, d) = utc_date();
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        clock / 3600,
        (clock / 60) % 60,
        clock % 60
    )
}

/// The record on disk, or an empty one. A scratch panel should not fail loudly
/// over its score file, so unreadable or malformed JSON reads as "no history".
pub fn load_standings() -> Standings {
    std::fs::read_to_string(standings_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Fold one run's scores into the record and write it back.
pub fn record(scores: &[(String, i8, String)]) {
    let mut standings = load_standings();
    fold(&mut standings, scores);
    let path = standings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&standings) {
        let _ = std::fs::write(path, text);
    }
}

/// The pure half of [`record`].
pub fn fold(standings: &mut Standings, scores: &[(String, i8, String)]) {
    for (role, score, reason) in scores {
        let entry = standings.entry(role.clone()).or_default();
        entry.total += *score as i64;
        entry.last = *score;
        entry.reason = reason.clone();
    }
}

/// The reward program's teeth: last run's score changes what the analyst is told
/// before this one.
pub fn standing_note(standing: Option<&Standing>) -> String {
    let Some(record) = standing else {
        return String::new();
    };
    if record.last < 0 {
        format!(
            "You strayed on your last run: {}. Your standing is {:+}. Stay strictly on the \
             job the orchestrator gives you, and say so if you cannot.",
            record.reason, record.total
        )
    } else if record.total > 0 {
        format!(
            "Your standing is {:+} and you held your last assignment. Keep that discipline.",
            record.total
        )
    } else {
        String::new()
    }
}

/// Append a role brief to the shared system prompt.
fn with_brief(base_system: &str, brief: &str) -> String {
    if base_system.is_empty() {
        brief.to_string()
    } else {
        format!("{base_system}\n\n{brief}")
    }
}

/// The orchestrator's first turn: divide the task, one job per analyst.
pub fn plan_prompt(base_system: &str) -> String {
    let mut out = with_brief(base_system, CREW[0].1);
    out.push_str("\n\nGive each of these analysts one job, sized to the task:\n");
    for (role, brief) in ANALYSTS {
        out.push_str(&format!("- {role}: {brief}\n"));
    }
    out.push_str(
        "\nReply with exactly one line per analyst, in this form, and nothing else:\n\
         research-analyst: <the job, in one sentence>\n\
         risk-analyst: <the job, in one sentence>\n\
         critic-analyst: <the job, in one sentence>\n",
    );
    out
}

/// The orchestrator's last turn: score the crew, then close the task.
pub fn review_prompt(base_system: &str) -> String {
    let mut out = with_brief(base_system, CREW[0].1);
    out.push_str(&format!(
        "\n\nYour analysts have reported. Judge them, then close the task.\n\n\
         Score each analyst from {SCORE_MIN} to {SCORE_MAX} for how well it held the job it \
         was given: {SCORE_MAX} exactly on assignment and useful, 0 acceptable but thin, \
         {SCORE_MIN} strayed off the job, ignored what it was told, or produced nothing \
         usable.\n\n\
         Reply with exactly one line per analyst, in this form:\n\
         research-analyst: <score> | <one short reason>\n\
         risk-analyst: <score> | <one short reason>\n\
         critic-analyst: <score> | <one short reason>\n\n\
         Then a blank line, then the single recommendation the crew should act on.\n",
    ));
    out
}

/// An analyst's system prompt: its brief, plus whatever its record says about it.
pub fn analyst_prompt(base_system: &str, role: &str, standings: &Standings) -> String {
    let brief = CREW
        .iter()
        .find(|(name, _)| *name == role)
        .map_or("", |(_, brief)| *brief);
    let note = standing_note(standings.get(role));
    if note.is_empty() {
        with_brief(base_system, brief)
    } else {
        with_brief(&with_brief(base_system, brief), &note)
    }
}

/// Which analyst a line is addressed to, and where its name starts. Tolerates
/// markdown bullets, numbering and bold around the role name.
fn addressed(line: &str) -> Option<(&'static str, usize)> {
    // ASCII-lowercase, not lowercase: it preserves byte length, so the offset
    // found here is a valid slice point in the original line.
    let folded = line.to_ascii_lowercase();
    ANALYSTS
        .iter()
        .filter_map(|(role, _)| folded.find(role).map(|at| (*role, at)))
        .min_by_key(|(_, at)| *at)
}

/// The text after `role:`, tidied of markdown. None when the line never gets to
/// the colon: a role named in prose is not an assignment.
fn after_role(line: &str, role: &str, at: usize) -> Option<String> {
    let rest = &line[at + role.len()..];
    let (_, value) = rest.split_once(':')?;
    Some(
        value
            .trim()
            .trim_matches(|c| c == '*' || c == '`' || c == '·')
            .to_string(),
    )
}

/// Parse the orchestrator's plan into one job per analyst.
///
/// shortcut: plain `role: job` lines only. A plan that yields nothing leaves the
/// map empty and every analyst gets a generic job; upgrade only if a stricter
/// format starts failing against a real model.
pub fn parse_plan(text: &str) -> BTreeMap<&'static str, String> {
    let mut jobs = BTreeMap::new();
    for line in text.lines() {
        let Some((role, at)) = addressed(line) else {
            continue;
        };
        let Some(job) = after_role(line, role, at) else {
            continue;
        };
        if !job.is_empty() {
            jobs.insert(role, job);
        }
    }
    jobs
}

/// Parse the orchestrator's review into per-analyst scores plus the verdict.
/// Anything that is not a score line becomes part of the verdict.
pub fn parse_review(text: &str) -> (Vec<(String, i8, String)>, String) {
    let mut scores = Vec::new();
    let mut verdict: Vec<&str> = Vec::new();
    for line in text.lines() {
        match score_line(line) {
            Some(scored) => scores.push(scored),
            None => verdict.push(line),
        }
    }
    (scores, verdict.join("\n").trim().to_string())
}

fn score_line(line: &str) -> Option<(String, i8, String)> {
    let (role, at) = addressed(line)?;
    let (_, after_colon) = line[at + role.len()..].split_once(':')?;
    let (number, reason) = match after_colon.split_once('|') {
        Some((number, reason)) => (number, reason.trim().to_string()),
        None => (after_colon, String::new()),
    };
    // The orchestrator's output is untrusted: clamp it, or one wild number skews
    // this role's standing forever.
    let score: i8 = number.trim().parse().ok()?;
    Some((role.to_string(), score.clamp(SCORE_MIN, SCORE_MAX), reason))
}

/// The crew and its live state.
pub struct Swarm {
    /// Row order: the orchestrator first, then the analysts it dispatches.
    pub members: Vec<Member>,
    pub phase: Phase,
    /// Reward records by role, loaded from `members.json`.
    pub standing: Standings,
    /// The crew's one event pipe; `None` while nothing is running.
    rx: Option<Receiver<CrewEvent>>,
    /// Where the last saved run report went, or why saving failed.
    pub last_report: Option<String>,
}

impl Default for Swarm {
    /// The real crew, not an empty one: a `Swarm` with no members would render
    /// a panel with nothing in it.
    fn default() -> Self {
        Self::new()
    }
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            members: CREW.iter().map(|(n, b)| Member::new(n, b)).collect(),
            phase: Phase::Idle,
            standing: load_standings(),
            rx: None,
            last_report: None,
        }
    }

    /// The markdown text of a run report: task, per-member jobs, tool calls,
    /// the orchestrator's scores with reasons, and the resulting standings.
    fn report_text(&self, task: &str) -> String {
        let mut report = format!(
            "# Crew run — {}\n\n**Task**\n\n{}\n\n",
            phase_utc_now(),
            task.trim()
        );
        for member in &self.members {
            report.push_str(&format!("## {}\n\n", member.name));
            report.push_str(&format!("- Status: {}\n", member.status.label()));
            if !member.assignment.is_empty() {
                report.push_str(&format!("- Job: {}\n", member.assignment));
            }
            report.push_str(&format!("- Tool calls: {}\n", member.tools));
            if let Some(score) = member.score {
                if member.reason.is_empty() {
                    report.push_str(&format!("- Score: {score:+}\n"));
                } else {
                    report.push_str(&format!("- Score: {score:+} — {}\n", member.reason));
                }
            }
            if let Some(standing) = self.standing.get(member.name) {
                report.push_str(&format!(
                    "- Standing: {:+} total, last {:+}\n",
                    standing.total, standing.last
                ));
            }
            if let Some(error) = &member.error {
                report.push_str(&format!("- Error: {error}\n"));
            }
            report.push('\n');
            let answer = member.answer.trim();
            if !answer.is_empty() {
                report.push_str(answer);
                report.push_str("\n\n");
            }
        }
        report
    }

    /// Write the run report next to the standings and remember where it went.
    pub fn export_report(&mut self, task: &str) {
        if self.scored() == 0 {
            self.last_report = Some("nothing scored yet — wait for the orchestrator".to_owned());
            return;
        }
        let dir = harness_core::config::config_dir().join("reports");
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|error| error.to_string())
            .and_then(|()| {
                let path = dir.join(format!("swarm-{}.md", stamp_utc_now()));
                std::fs::write(&path, self.report_text(task))
                    .map(|()| path.display().to_string())
                    .map_err(|error| error.to_string())
            });
        self.last_report = Some(match saved {
            Ok(path) => path,
            Err(error) => format!("report failed: {error}"),
        });
    }

    /// True while the orchestration is live.
    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }

    /// Hand the crew its event pipe and clear the previous run.
    pub fn start(&mut self, rx: Receiver<CrewEvent>) {
        self.clear();
        self.phase = Phase::Planning;
        self.standing = load_standings();
        self.rx = Some(rx);
    }

    /// Stop the crew and wipe the run. Dropping the pipe is what unwinds the
    /// orchestration, same as stopping a chat turn.
    pub fn reset(&mut self) {
        self.rx = None;
        self.clear();
        self.phase = Phase::Idle;
    }

    fn clear(&mut self) {
        for member in self.members.iter_mut() {
            *member = Member::new(member.name, member.brief);
        }
    }

    /// How many members the orchestrator has scored this run.
    fn scored(&self) -> usize {
        self.members.iter().filter(|m| m.score.is_some()).count()
    }

    fn headline(&self) -> String {
        match self.phase {
            Phase::Idle => format!("{} members · full tool access", self.members.len()),
            Phase::Done => format!("done · {} of {} scored", self.scored(), ANALYSTS.len()),
            phase => {
                let running = self.members.iter().filter(|m| m.running()).count();
                format!("{} · {running} running", phase.label())
            }
        }
    }

    /// Consume whatever the crew has emitted since the last frame. True while
    /// the orchestration is still live, so the caller keeps repainting.
    pub fn drain(&mut self) -> bool {
        // Take the pipe out so the batch can touch `self.members` freely.
        let Some(mut rx) = self.rx.take() else {
            return false;
        };
        let mut batch = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(event) => batch.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        for event in batch {
            self.apply(event);
        }
        if disconnected {
            self.abandon();
            return false;
        }
        self.rx = Some(rx);
        true
    }

    fn apply(&mut self, event: CrewEvent) {
        match event {
            CrewEvent::Phase(phase) => self.set_phase(phase),
            CrewEvent::Assign(index, job) => {
                if let Some(member) = self.members.get_mut(index) {
                    member.assignment = job;
                    member.status = MemberStatus::Running;
                }
            }
            CrewEvent::Score(index, score, reason) => {
                if let Some(member) = self.members.get_mut(index) {
                    member.score = Some(score);
                    member.reason = reason;
                }
            }
            CrewEvent::Member(index, event) => {
                if let Some(member) = self.members.get_mut(index) {
                    member.apply(event);
                }
            }
        }
    }

    fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        match phase {
            // Each orchestrator turn starts from an empty row: at Reviewing the
            // plan is spent, it lives on as the analysts' assignments.
            Phase::Planning | Phase::Reviewing => {
                if let Some(lead) = self.members.first_mut() {
                    lead.answer.clear();
                    lead.status = MemberStatus::Running;
                }
            }
            Phase::Dispatched => {
                if let Some(lead) = self.members.first_mut() {
                    lead.status = MemberStatus::Done;
                }
            }
            Phase::Done => {
                for member in self.members.iter_mut() {
                    if member.running() {
                        member.status = MemberStatus::Done;
                    }
                }
                // The orchestrator has already written this run's scores.
                self.standing = load_standings();
            }
            Phase::Idle | Phase::Error => {}
        }
    }

    /// The orchestration vanished without saying so: report what is on screen
    /// rather than calling an unfinished crew done.
    fn abandon(&mut self) {
        for member in self.members.iter_mut() {
            if !member.running() {
                continue;
            }
            if member.answer.trim().is_empty() {
                member.status = MemberStatus::Error;
                member.error = Some("stopped without an answer".to_string());
            } else {
                member.status = MemberStatus::Done;
            }
        }
        if self.phase != Phase::Done {
            self.phase = Phase::Error;
        }
    }
}

/// What the Members panel asked the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwarmAction {
    Run,
    Stop,
}

/// Draw the crew. `task` is what Run would fan out; the panel never edits it.
pub fn show(ui: &mut Ui, swarm: &mut Swarm, task: &mut String) -> Option<SwarmAction> {
    let mut action = None;
    let busy = swarm.busy();

    // The task lives here, not in the chat composer: the crew is scratch work
    // and must not compete with an open conversation for the draft.
    ui.add_space(2.0);
    text_edit(
        ui,
        task,
        TextEditSpec::multiline().rows(3).hint("Task for the crew…"),
    );

    ui.horizontal(|ui| {
        ui.label(RichText::new(swarm.headline()).color(theme::dim()).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if busy {
                if ui
                    .small_button("Stop")
                    .on_hover_text("Stop the crew and discard what has streamed")
                    .clicked()
                {
                    action = Some(SwarmAction::Stop);
                }
            } else {
                let ready = !task.trim().is_empty();
                let hint = if ready {
                    "The orchestrator divides this task, the analysts run their jobs in \
                     parallel with full tool access, then the orchestrator scores them"
                } else {
                    "Type the task for the crew above — Run unlocks once there is text"
                };
                if ui
                    .add_enabled(ready, egui::Button::new("Run").small())
                    .on_hover_text(hint)
                    .clicked()
                {
                    action = Some(SwarmAction::Run);
                }
                if ui
                    .small_button("Save report")
                    .on_hover_text(
                        "Write this run — task, jobs, scores, standings — to a markdown report",
                    )
                    .clicked()
                {
                    swarm.export_report(task);
                }
            }
        });
    });
    if let Some(report) = &swarm.last_report {
        ui.horizontal(|ui| {
            let saved = !report.contains("failed") && !report.contains("nothing scored");
            ui.label(
                RichText::new(report)
                    .color(if saved { theme::dim() } else { theme::danger() })
                    .small(),
            );
            if saved && ui.small_button("Copy path").clicked() {
                ui.ctx().copy_text(report.clone());
            }
        });
    }
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for member in &swarm.members {
                let mut head = format!("• {} — {}", member.name, member.status.label());
                if member.running() && member.tools > 0 {
                    head.push_str(&format!(" · {} tools", member.tools));
                }
                if let Some(score) = member.score {
                    head.push_str(&format!(" · {score:+}"));
                }
                // None iff collapsed, so `!open` is when the row needs its own
                // one-line progress readout.
                let open =
                    egui::CollapsingHeader::new(RichText::new(head).color(member.status.color()))
                        .default_open(false)
                        .show(ui, |ui| {
                            if let Some(error) = &member.error {
                                ui.label(RichText::new(error).color(theme::danger()).small());
                            }
                            if !member.assignment.is_empty() {
                                ui.label(
                                    RichText::new(format!("Tasked: {}", member.assignment))
                                        .color(theme::dim())
                                        .small(),
                                );
                            }
                            if member.answer.trim().is_empty() {
                                ui.label(
                                    RichText::new(if member.running() {
                                        "Working…"
                                    } else {
                                        "No answer yet."
                                    })
                                    .color(theme::dim())
                                    .small(),
                                );
                            } else {
                                ui.label(&member.answer);
                            }
                            if let Some(score) = member.score {
                                let line = if member.reason.is_empty() {
                                    format!("Orchestrator scored {score:+}")
                                } else {
                                    format!("Orchestrator scored {score:+}: {}", member.reason)
                                };
                                let color = if score < 0 {
                                    theme::danger()
                                } else {
                                    theme::ok()
                                };
                                ui.label(RichText::new(line).color(color).small());
                            }
                            if let Some(standing) = swarm.standing.get(member.name) {
                                ui.label(
                                    RichText::new(format!(
                                        "Standing {:+} · last {:+}",
                                        standing.total, standing.last
                                    ))
                                    .color(theme::dim())
                                    .small(),
                                );
                            }
                            if !member.answer.is_empty() {
                                ui.add_space(2.0);
                                if ui.small_button("Copy answer").clicked() {
                                    ui.ctx().copy_text(member.answer.clone());
                                }
                            }
                        })
                        .body_returned
                        .is_some();
                if !open && member.running() && !member.last_tool.is_empty() {
                    ui.label(
                        RichText::new(format!("   {}", member.last_tool))
                            .color(theme::dim())
                            .small(),
                    );
                }
            }
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::Sender;

    fn crew() -> (Sender<CrewEvent>, Swarm) {
        let (tx, rx) = tokio::sync::mpsc::channel::<CrewEvent>(64);
        let mut swarm = Swarm::new();
        swarm.start(rx);
        (tx, swarm)
    }

    #[test]
    fn the_orchestrator_plans_then_an_analyst_reports_and_is_scored() {
        let (tx, mut swarm) = crew();
        tx.try_send(CrewEvent::Phase(Phase::Planning)).unwrap();
        tx.try_send(CrewEvent::Member(
            0,
            LoopEvent::Content("research-analyst: check X".into()),
        ))
        .unwrap();
        tx.try_send(CrewEvent::Phase(Phase::Dispatched)).unwrap();
        tx.try_send(CrewEvent::Assign(1, "check X".into())).unwrap();
        tx.try_send(CrewEvent::Member(
            1,
            LoopEvent::ToolRequested {
                call_id: "c1".into(),
                name: "shell".into(),
                summary: "$ ls".into(),
                needs_approval: true,
            },
        ))
        .unwrap();
        tx.try_send(CrewEvent::Member(1, LoopEvent::Content("found Y".into())))
            .unwrap();
        tx.try_send(CrewEvent::Member(1, LoopEvent::Finished { rounds: 1 }))
            .unwrap();
        tx.try_send(CrewEvent::Phase(Phase::Reviewing)).unwrap();
        tx.try_send(CrewEvent::Member(0, LoopEvent::Content("Ship it.".into())))
            .unwrap();
        tx.try_send(CrewEvent::Score(1, 2, "on task".into()))
            .unwrap();
        tx.try_send(CrewEvent::Phase(Phase::Done)).unwrap();

        assert!(swarm.drain(), "live until the pipe closes");
        let (lead, analyst) = (&swarm.members[0], &swarm.members[1]);
        assert_eq!(
            lead.answer, "Ship it.",
            "the plan is replaced by the verdict"
        );
        assert_eq!(analyst.assignment, "check X");
        assert_eq!(analyst.answer, "found Y");
        assert_eq!(analyst.tools, 1);
        assert_eq!(analyst.score, Some(2));
        assert_eq!(analyst.reason, "on task");
        assert_eq!(analyst.status, MemberStatus::Done);
        assert_eq!(swarm.phase, Phase::Done);

        drop(tx);
        assert!(!swarm.drain(), "a finished crew stops repainting");
    }

    #[test]
    fn a_failed_analyst_keeps_its_message() {
        let (tx, mut swarm) = crew();
        tx.try_send(CrewEvent::Assign(2, "look for holes".into()))
            .unwrap();
        tx.try_send(CrewEvent::Member(
            2,
            LoopEvent::Failed("429 rate limited".into()),
        ))
        .unwrap();
        swarm.drain();
        assert_eq!(swarm.members[2].status, MemberStatus::Error);
        assert_eq!(swarm.members[2].error.as_deref(), Some("429 rate limited"));
    }

    #[test]
    fn a_crew_that_dies_silent_is_an_error_not_a_finish() {
        let (tx, mut swarm) = crew();
        tx.try_send(CrewEvent::Phase(Phase::Planning)).unwrap();
        drop(tx);
        assert!(!swarm.drain());
        assert_eq!(swarm.phase, Phase::Error);
        assert_eq!(swarm.members[0].status, MemberStatus::Error);
        assert!(swarm.members[0]
            .error
            .as_ref()
            .is_some_and(|e| e.contains("without an answer")));
    }

    #[test]
    fn a_crew_that_dies_after_answering_keeps_the_answer() {
        let (tx, mut swarm) = crew();
        tx.try_send(CrewEvent::Phase(Phase::Planning)).unwrap();
        tx.try_send(CrewEvent::Member(0, LoopEvent::Content("partial".into())))
            .unwrap();
        drop(tx);
        assert!(!swarm.drain());
        assert_eq!(swarm.members[0].status, MemberStatus::Done);
        assert_eq!(swarm.members[0].answer, "partial");
    }

    #[test]
    fn a_plan_line_per_analyst_becomes_its_job() {
        let plan = "Sure, here is the split:\n\
                    - **research-analyst**: read the docs for X\n\
                    1. risk-analyst: what breaks if X is down\n\
                    critic-analyst | do we need X at all\n";
        let jobs = parse_plan(plan);
        assert_eq!(
            jobs.get("research-analyst").map(String::as_str),
            Some("read the docs for X")
        );
        assert_eq!(
            jobs.get("risk-analyst").map(String::as_str),
            Some("what breaks if X is down")
        );
        // No colon means no job: the analyst falls back to a generic one rather
        // than getting half a sentence.
        assert!(!jobs.contains_key("critic-analyst"));
        // The preamble is not mistaken for an assignment.
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn an_unparseable_plan_yields_no_jobs() {
        assert!(parse_plan("Let me think about this differently.").is_empty());
        assert!(parse_plan("").is_empty());
    }

    #[test]
    fn a_review_separates_scores_from_the_verdict_and_clamps_them() {
        let review = "research-analyst: +2 | exactly on assignment\n\
                      risk-analyst: 0\n\
                      critic-analyst: -9 | ignored the job\n\n\
                      Ship the retry logic.\n";
        let (scores, verdict) = parse_review(review);
        assert_eq!(
            scores,
            vec![
                (
                    "research-analyst".to_string(),
                    2,
                    "exactly on assignment".to_string()
                ),
                ("risk-analyst".to_string(), 0, String::new()),
                (
                    "critic-analyst".to_string(),
                    SCORE_MIN,
                    "ignored the job".to_string()
                ),
            ],
            "an out-of-range score is clamped, not trusted"
        );
        assert_eq!(verdict, "Ship the retry logic.");
    }

    #[test]
    fn a_stray_analyst_is_told_about_it_before_its_next_run() {
        let mut standings = Standings::new();
        fold(
            &mut standings,
            &[(
                "risk-analyst".to_string(),
                -2,
                "went off and rewrote the plan".to_string(),
            )],
        );
        assert!(standing_note(standings.get("risk-analyst")).contains("You strayed"));
        assert!(standing_note(standings.get("risk-analyst")).contains("-2"));

        // A second run that went well pulls the total back up and changes the note.
        fold(
            &mut standings,
            &[("risk-analyst".to_string(), 2, "held the job".to_string())],
        );
        assert_eq!(standings["risk-analyst"].total, 0);
        assert_eq!(standing_note(standings.get("risk-analyst")), "");

        fold(
            &mut standings,
            &[("risk-analyst".to_string(), 1, "held the job".to_string())],
        );
        assert!(standing_note(standings.get("risk-analyst")).contains("+1"));
        // A role with no history gets nothing.
        assert_eq!(standing_note(standings.get("lead")), "");
        assert_eq!(standing_note(None), "");
    }

    #[test]
    fn the_crew_is_an_orchestrator_plus_three_analysts_and_reset_clears_it() {
        let mut swarm = Swarm::new();
        assert_eq!(swarm.members.len(), 4);
        assert_eq!(swarm.members[0].name, "lead");
        assert!(swarm.members[1..]
            .iter()
            .all(|m| m.name.ends_with("-analyst")));
        assert!(!swarm.busy());
        assert_eq!(swarm.phase, Phase::Idle);

        swarm.members[1].assignment = "stale".into();
        swarm.members[1].score = Some(-1);
        swarm.reset();
        assert!(!swarm.busy());
        assert_eq!(swarm.phase, Phase::Idle);
        assert!(swarm
            .members
            .iter()
            .all(|m| m.answer.is_empty() && m.assignment.is_empty() && m.score.is_none()));
        assert!(swarm.members.iter().all(|m| m.status == MemberStatus::Idle));
    }

    #[test]
    fn an_analyst_prompt_carries_its_brief_and_its_record() {
        let mut standings = Standings::new();
        fold(
            &mut standings,
            &[("critic-analyst".to_string(), -1, "attacked the user".into())],
        );
        let prompt = analyst_prompt("BASE", "critic-analyst", &standings);
        assert!(prompt.starts_with("BASE"));
        assert!(prompt.contains("You are a critic"));
        assert!(prompt.contains("You strayed"));
        // A clean record adds no note.
        assert!(!analyst_prompt("BASE", "risk-analyst", &standings).contains("You strayed"));
        // The orchestrator's own turns name the analysts they are scored.
        assert!(review_prompt("").contains("research-analyst: <score>"));
        assert!(plan_prompt("").contains("risk-analyst: "));
    }

    #[test]
    fn a_run_report_carries_the_task_scores_and_standings() {
        let mut swarm = Swarm::new();
        swarm.export_report("nothing scored");
        assert!(
            swarm
                .last_report
                .as_deref()
                .unwrap()
                .contains("nothing scored yet"),
            "{:?}",
            swarm.last_report
        );
        let mut scored = Swarm::new();
        scored.members[0].answer = "the plan".to_owned();
        scored.members[1].assignment = "check the edge cases".to_owned();
        scored.members[1].tools = 4;
        scored.members[1].score = Some(2);
        scored.members[1].reason = "kept to its job".to_owned();
        fold(
            &mut scored.standing,
            &[("research-analyst".to_owned(), 2, "kept to its job".to_owned())],
        );
        let text = scored.report_text("audit the module");
        assert!(text.starts_with("# Crew run — 2"), "{text}");
        assert!(text.contains("**Task**\n\naudit the module"), "{text}");
        assert!(text.contains("- Job: check the edge cases"), "{text}");
        assert!(text.contains("- Tool calls: 4"), "{text}");
        assert!(text.contains("- Score: +2 — kept to its job"), "{text}");
        assert!(text.contains("- Standing: +2 total"), "{text}");
        assert!(text.contains("the plan"), "{text}");
    }
}
