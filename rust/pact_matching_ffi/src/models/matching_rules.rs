//! Rules defining how matching is performed.

use crate::util::*;
use crate::{as_ref, ffi_fn, safe_str};
use anyhow::Context as _;
use libc::c_char;
use pact_matching::models::matchingrules::RuleLogic as NonCRuleLogic;
use std::convert::{From, Into};

pub use pact_matching::models::matchingrules::MatchingRuleCategory;

ffi_fn! {
    /// Get a new empty `MatchingRuleCategory` with the given name.
    fn matching_rule_category_new_empty(name: *const c_char) -> *mut MatchingRuleCategory {
        let name = safe_str!(name);
        ptr::raw_to(MatchingRuleCategory::empty(name))
    } {
        ptr::null_mut_to::<MatchingRuleCategory>()
    }
}

ffi_fn! {
    /// Get a new equality-matching `MatchingRuleCategory` with the given name.
    fn matching_rule_category_new_equality(name: *const c_char) -> *mut MatchingRuleCategory {
        let name = safe_str!(name);
        ptr::raw_to(MatchingRuleCategory::equality(name))
    } {
        ptr::null_mut_to::<MatchingRuleCategory>()
    }
}

ffi_fn! {
    /// Check if the `MatchingRuleCategory` is empty.
    fn matching_rule_category_is_empty(mr_cat: *const MatchingRuleCategory) -> bool {
        let mr_cat = as_ref!(mr_cat);
        mr_cat.is_empty()
    } {
        false
    }
}

ffi_fn! {
    /// Check if the `MatchingRuleCategory` is not empty.
    fn matching_rule_category_is_not_empty(mr_cat: *const MatchingRuleCategory) -> bool {
        let mr_cat = as_ref!(mr_cat);
        mr_cat.is_not_empty()
    } {
        false
    }
}

/// Define how to combine rules.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum RuleLogic {
    /// All rules must match.
    And,
    /// At least one rule must match.
    Or,
}

impl From<NonCRuleLogic> for RuleLogic {
    #[inline]
    fn from(other: NonCRuleLogic) -> RuleLogic {
        match other {
            NonCRuleLogic::And => RuleLogic::And,
            NonCRuleLogic::Or => RuleLogic::Or,
        }
    }
}

impl Into<NonCRuleLogic> for RuleLogic {
    #[inline]
    fn into(self) -> NonCRuleLogic {
        match self {
            RuleLogic::And => NonCRuleLogic::And,
            RuleLogic::Or => NonCRuleLogic::Or,
        }
    }
}
