/// Construct a `Value::Map` from key-value pairs, calling `.into_masque_value()`
/// on each value.
///
/// # Example
/// ```ignore
/// use masque::{masque_map, runtime::value::Value};
///
/// let val = masque_map! {
///     "name" => "Alice",
///     "age" => 30,
///     "active" => true,
/// };
/// ```
#[macro_export]
macro_rules! masque_map {
    ($($key:expr => $value:expr),* $(,)?) => {{
        use $crate::runtime::value::IntoMasqueValue;
        let mut map = ::std::collections::HashMap::new();
        $(
            map.insert($key.into(), IntoMasqueValue::into_masque_value($value));
        )*
        $crate::runtime::value::Value::Map(map)
    }};
}
