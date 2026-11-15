//! Channel names: `meeting:{id}` and `team:{id}` (spec §11.1).

use std::{fmt, str::FromStr};

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Meeting(Uuid),
    Team(Uuid),
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Meeting(id) => write!(f, "meeting:{id}"),
            Self::Team(id) => write!(f, "team:{id}"),
        }
    }
}

impl FromStr for Channel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        let (kind, id) = s.split_once(':').ok_or(())?;
        let id = Uuid::parse_str(id).map_err(|_| ())?;
        match kind {
            "meeting" => Ok(Self::Meeting(id)),
            "team" => Ok(Self::Team(id)),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let id = Uuid::now_v7();
        for channel in [Channel::Meeting(id), Channel::Team(id)] {
            assert_eq!(channel.to_string().parse::<Channel>(), Ok(channel));
        }
    }

    #[test]
    fn rejects_unknown_kinds_and_ids() {
        assert!(
            "user:0193a0c0-0000-7000-8000-000000000000"
                .parse::<Channel>()
                .is_err()
        );
        assert!("meeting:nope".parse::<Channel>().is_err());
        assert!("meeting".parse::<Channel>().is_err());
    }
}
