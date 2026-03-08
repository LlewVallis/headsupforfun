#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingProfile {
    Smoke,
    Dev,
    Full,
}

impl TrainingProfile {
    pub const fn total_iterations(self) -> u64 {
        match self {
            Self::Smoke => 2_000,
            Self::Dev => 8_000,
            Self::Full => 25_000,
        }
    }

    pub const fn checkpoint_interval(self) -> u64 {
        match self {
            Self::Smoke => 500,
            Self::Dev => 2_000,
            Self::Full => 5_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrainingProfile;

    #[test]
    fn profiles_increase_work_monotonically() {
        assert!(TrainingProfile::Smoke.total_iterations() < TrainingProfile::Dev.total_iterations());
        assert!(TrainingProfile::Dev.total_iterations() < TrainingProfile::Full.total_iterations());
        assert!(TrainingProfile::Smoke.checkpoint_interval() <= TrainingProfile::Smoke.total_iterations());
    }
}
