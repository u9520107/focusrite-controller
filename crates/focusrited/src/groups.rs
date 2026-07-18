//! Mock-first validation and normalized mapping for future virtual level groups.

use std::collections::BTreeSet;

use crate::{ControlCapability, ControlId, ServiceError, ValueDomain};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelGroup {
    pub members: Vec<ControlId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupError {
    TooFewMembers,
    DuplicateMember,
    IneligibleMember(ControlId),
    InvalidPosition,
    InvalidRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupResult {
    pub applied: Vec<ControlId>,
    pub failed: Option<(ControlId, ServiceError)>,
}

/// Maps canonical 0..=1000 position into a declared integer range.
pub fn map_level(position: u16, minimum: i32, maximum: i32) -> Result<i32, GroupError> {
    if position > 1000 {
        return Err(GroupError::InvalidPosition);
    }
    if minimum > maximum {
        return Err(GroupError::InvalidRange);
    }
    Ok((i64::from(minimum)
        + ((i64::from(maximum) - i64::from(minimum)) * i64::from(position) + 500) / 1000)
        as i32)
}

impl LevelGroup {
    pub fn validate(&self, capabilities: &[ControlCapability]) -> Result<(), GroupError> {
        if self.members.len() < 2 {
            return Err(GroupError::TooFewMembers);
        }
        let mut seen = BTreeSet::new();
        for member in &self.members {
            if !seen.insert(member) {
                return Err(GroupError::DuplicateMember);
            }
            let valid = capabilities.iter().any(|capability| {
                capability.id == *member
                    && capability.available
                    && capability.writable
                    && capability.domain == ValueDomain::Integer
                    && capability.minimum.is_some()
                    && capability.maximum.is_some()
                    && capability.minimum <= capability.maximum
            });
            if !valid {
                return Err(GroupError::IneligibleMember(member.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ControlCapability;

    fn level(id: &str, minimum: i32, maximum: i32) -> ControlCapability {
        ControlCapability {
            id: ControlId(id.into()),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(minimum),
            maximum: Some(maximum),
            presentation: None,
        }
    }

    #[test]
    fn validates_leaf_levels_and_maps_unequal_ranges() {
        let capabilities = vec![level("a", 0, 100), level("b", 20, 220)];
        let group = LevelGroup {
            members: vec![ControlId("a".into()), ControlId("b".into())],
        };
        assert_eq!(group.validate(&capabilities), Ok(()));
        assert_eq!(map_level(500, 0, 100), Ok(50));
        assert_eq!(map_level(500, 20, 220), Ok(120));
    }

    #[test]
    fn rejects_duplicate_or_ineligible_member() {
        let capabilities = vec![level("a", 0, 100)];
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("a".into())]
            }
            .validate(&capabilities),
            Err(GroupError::DuplicateMember)
        );
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("missing".into())]
            }
            .validate(&capabilities),
            Err(GroupError::IneligibleMember(ControlId("missing".into())))
        );
    }

    #[test]
    fn rejects_inverted_bounds_before_mapping() {
        let capabilities = vec![level("a", 100, 0), level("b", 0, 100)];
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("b".into())]
            }
            .validate(&capabilities),
            Err(GroupError::IneligibleMember(ControlId("a".into())))
        );
        assert_eq!(map_level(500, 100, 0), Err(GroupError::InvalidRange));
    }
}
