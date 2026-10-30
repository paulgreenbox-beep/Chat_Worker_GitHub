//! Canonical journal transitions. Keep the writer's existing string form;
//! never accept externally tagged object/null aliases while replaying.
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Transition {
    Begin,
    Accepted,
    Unknown,
    Acknowledge,
    Complete,
    Block,
}

impl<'de> Deserialize<'de> for Transition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TransitionVisitor;
        impl Visitor<'_> for TransitionVisitor {
            type Value = Transition;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a canonical uppercase journal transition string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Transition, E> {
                match value {
                    "BEGIN" => Ok(Transition::Begin),
                    "ACCEPTED" => Ok(Transition::Accepted),
                    "UNKNOWN" => Ok(Transition::Unknown),
                    "ACKNOWLEDGE" => Ok(Transition::Acknowledge),
                    "COMPLETE" => Ok(Transition::Complete),
                    "BLOCK" => Ok(Transition::Block),
                    _ => Err(E::custom("invalid journal transition")),
                }
            }
        }
        deserializer.deserialize_str(TransitionVisitor)
    }
}
