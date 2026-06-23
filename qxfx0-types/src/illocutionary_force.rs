use serde::{Deserialize, Serialize};

/// 5 IllocutionaryForce — speech act type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IllocutionaryForce {
    IFAssert,
    IFOffer,
    IFAsk,
    IFConfront,
    IFContact,
}
