//! Mock-first validation and normalized mapping for future virtual level groups.

use std::collections::BTreeSet;

use crate::{ControlCapability, ControlId, GroupOperation, ServiceError, ValueDomain};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelGroup {
    pub members: Vec<ControlId>,
    /// Member whose normalized position receives a group command target.
    pub anchor: ControlId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupError {
    TooFewMembers,
    DuplicateMember,
    InvalidAnchor,
    IneligibleMember(ControlId),
    InvalidPosition,
    InvalidRange,
    UnmappableCurrentState(ControlId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupResult {
    pub applied: Vec<ControlId>,
    pub skipped: Vec<ControlId>,
    pub failed: Option<(ControlId, ServiceError)>,
}

/// One confirmed source-to-target level mapping result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirrorResult {
    pub source: ControlId,
    pub target: ControlId,
    pub applied: bool,
    pub skipped: bool,
    pub deferred: bool,
    pub failed: Option<ServiceError>,
}

/// Maps canonical 0..=1000 position into a declared integer range.
pub fn map_level(position: u16, minimum: i32, maximum: i32) -> Result<i32, GroupError> {
    if position > 1000 {
        return Err(GroupError::InvalidPosition);
    }
    if minimum >= maximum {
        return Err(GroupError::InvalidRange);
    }
    Ok((i64::from(minimum)
        + ((i64::from(maximum) - i64::from(minimum)) * i64::from(position) + 500) / 1000)
        as i32)
}

/// Maps a declared integer value to canonical 0..=1000 position.
pub fn unmap_level(value: i32, minimum: i32, maximum: i32) -> Result<u16, GroupError> {
    if minimum >= maximum {
        return Err(GroupError::InvalidRange);
    }
    if value < minimum || value > maximum {
        return Err(GroupError::InvalidPosition);
    }
    let span = i64::from(maximum) - i64::from(minimum);
    Ok((((i64::from(value) - i64::from(minimum)) * 1000 + span / 2) / span) as u16)
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
            validate_relative_level(member, capabilities)?;
        }
        if !self.members.contains(&self.anchor) {
            return Err(GroupError::InvalidAnchor);
        }
        Ok(())
    }
}

/// Validates one adapter-declared level control for a normalized operation.
pub fn validate_relative_level(
    control: &ControlId,
    capabilities: &[ControlCapability],
) -> Result<(), GroupError> {
    capabilities
        .iter()
        .any(|capability| {
            capability.id == *control
                && capability.available
                && capability.writable
                && capability.domain == ValueDomain::Integer
                && capability
                    .group
                    .as_ref()
                    .is_some_and(|group| group.operation == GroupOperation::RelativeLevel)
                && capability.minimum.is_some()
                && capability.maximum.is_some()
                && capability
                    .minimum
                    .zip(capability.maximum)
                    .is_some_and(|(minimum, maximum)| minimum < maximum)
        })
        .then_some(())
        .ok_or_else(|| GroupError::IneligibleMember(control.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControlCapability, GroupCapability};

    fn level(id: &str, minimum: i32, maximum: i32) -> ControlCapability {
        ControlCapability {
            id: ControlId(id.into()),
            domain: ValueDomain::Integer,
            writable: true,
            available: true,
            minimum: Some(minimum),
            maximum: Some(maximum),
            group: Some(GroupCapability {
                operation: GroupOperation::RelativeLevel,
            }),
            presentation: None,
        }
    }

    #[test]
    fn validates_leaf_levels_and_maps_unequal_ranges() {
        let capabilities = vec![level("a", 0, 100), level("b", 20, 220)];
        let group = LevelGroup {
            members: vec![ControlId("a".into()), ControlId("b".into())],
            anchor: ControlId("a".into()),
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
                members: vec![ControlId("a".into()), ControlId("a".into())],
                anchor: ControlId("a".into()),
            }
            .validate(&capabilities),
            Err(GroupError::DuplicateMember)
        );
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("missing".into())],
                anchor: ControlId("a".into()),
            }
            .validate(&capabilities),
            Err(GroupError::IneligibleMember(ControlId("missing".into())))
        );
    }

    #[test]
    fn rejects_control_without_adapter_group_declaration() {
        let mut undeclared = level("a", 0, 100);
        undeclared.group = None;
        let capabilities = vec![undeclared, level("b", 0, 100)];
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("b".into())],
                anchor: ControlId("a".into()),
            }
            .validate(&capabilities),
            Err(GroupError::IneligibleMember(ControlId("a".into())))
        );
    }

    #[test]
    fn rejects_inverted_bounds_before_mapping() {
        let capabilities = vec![level("a", 100, 0), level("b", 0, 100)];
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("b".into())],
                anchor: ControlId("a".into()),
            }
            .validate(&capabilities),
            Err(GroupError::IneligibleMember(ControlId("a".into())))
        );
        assert_eq!(map_level(500, 100, 0), Err(GroupError::InvalidRange));
    }

    #[test]
    fn requires_anchor_and_round_trips_positions() {
        let capabilities = vec![level("a", 0, 100), level("b", 20, 220)];
        assert_eq!(
            LevelGroup {
                members: vec![ControlId("a".into()), ControlId("b".into())],
                anchor: ControlId("missing".into()),
            }
            .validate(&capabilities),
            Err(GroupError::InvalidAnchor)
        );
        assert_eq!(unmap_level(50, 0, 100), Ok(500));
        assert_eq!(unmap_level(120, 20, 220), Ok(500));
    }
}
