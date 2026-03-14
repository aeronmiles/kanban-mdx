//! Shared serde helper predicates for `skip_serializing_if` attributes.

pub fn is_zero(v: &i32) -> bool {
    *v == 0
}

pub fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

pub fn is_false(v: &bool) -> bool {
    !v
}
