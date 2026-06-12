use nightshade::prelude::serde::{Deserialize, Serialize};

/// Marker component for viewer-specific entities.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "nightshade::prelude::serde")]
pub struct Marker;
