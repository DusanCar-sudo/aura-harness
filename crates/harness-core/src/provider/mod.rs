//! Universal multi-provider layer, ported from aura-code (TypeScript → Rust).
//!
//! Each provider has a descriptor: family, routing prefixes, API key env var,
//! transport class, and base URL. Model names route to the matching provider
//! through that one table.

pub mod descriptors;
pub mod factory;
pub mod transport;

pub use descriptors::{TransportClass, ProviderDescriptor, KNOWN_PROVIDERS, api_env_for, detect_provider_kind, get_api_key_for_model, base_url_for, provider_family, strip_routing_prefix};