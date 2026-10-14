//! Multi-provider routing: model name → provider → transport.

use super::descriptors::{TransportClass};

pub fn detect_provider_kind(model_id: &str) -> TransportClass {
    super::descriptors::detect_provider_kind(model_id)
}