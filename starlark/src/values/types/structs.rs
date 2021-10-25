/*
 * Copyright 2018 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The struct type, an associative-map created with `struct()`.
//!
//! This struct type is related to both the [dictionary](crate::values::dict) and the
//! [record](crate::values::record) types, all being associative maps.
//!
//! * Like a record, a struct is immutable, fields can be referred to with `struct.field`, and
//!   it uses strings for keys.
//! * Like a dictionary, the struct is untyped, and manipulating structs from Rust is ergonomic.
//!
//! The `struct()` function creates a struct. It accepts keyword arguments, keys become
//! struct field names, and values become field values.
//!
//! ```
//! # starlark::assert::is_true(r#"
//! ip_address = struct(host='localhost', port=80)
//! ip_address.port == 80
//! # "#);
//! ```

use std::{
    cmp::Ordering,
    fmt::{self, Display},
    hash::Hash,
    marker,
    marker::PhantomData,
};

use gazebo::{
    any::AnyLifetime,
    coerce::{coerce_ref, Coerce},
};

use crate as starlark;
use crate::{
    collections::{SmallMap, StarlarkHasher},
    environment::{Globals, GlobalsStatic},
    values::{
        comparison::{compare_small_map, equals_small_map},
        error::ValueError,
        AllocValue, Freeze, Freezer, FrozenValue, Heap, StarlarkValue, StringValue,
        StringValueLike, Trace, UnpackValue, Value, ValueLike, ValueOf,
    },
};

impl<'v, V: ValueLike<'v>> StructGen<'v, V> {
    /// The result of calling `type()` on a struct.
    pub const TYPE: &'static str = "struct";

    /// Create a new [`Struct`].
    pub fn new(fields: SmallMap<V::String, V>) -> Self {
        Self {
            fields,
            _marker: marker::PhantomData,
        }
    }
}

starlark_complex_value!(pub Struct<'v>);

/// The result of calling `struct()`.
#[derive(Clone, Default, Debug, Trace)]
#[repr(C)]
pub struct StructGen<'v, V: ValueLike<'v>> {
    /// The fields in a struct.
    pub fields: SmallMap<V::String, V>,
    _marker: marker::PhantomData<&'v String>,
}

unsafe impl<'v> Coerce<StructGen<'v, Value<'v>>> for StructGen<'static, FrozenValue> {}

impl<'v, V: ValueLike<'v>> Display for StructGen<'v, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "struct(")?;
        for (i, (name, value)) in self.fields.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}=", name.to_string_value().as_str())?;
            Display::fmt(value, f)?;
        }
        write!(f, ")")
    }
}

/// A builder to create a `Struct` easily.
pub struct StructBuilder<'v>(&'v Heap, SmallMap<StringValue<'v>, Value<'v>>);

impl<'v> StructBuilder<'v> {
    /// Create a new [`StructBuilder`] with a given capacity.
    pub fn with_capacity(heap: &'v Heap, capacity: usize) -> Self {
        Self(heap, SmallMap::with_capacity(capacity))
    }

    /// Create a new [`StructBuilder`].
    pub fn new(heap: &'v Heap) -> Self {
        Self(heap, SmallMap::new())
    }

    /// Add an element to the underlying [`Struct`].
    pub fn add(&mut self, key: &str, val: impl AllocValue<'v>) {
        self.1
            .insert(self.0.alloc_string_value(key), self.0.alloc(val));
    }

    /// Finish building and produce a [`Struct`].
    pub fn build(self) -> Struct<'v> {
        Struct {
            fields: self.1,
            _marker: marker::PhantomData,
        }
    }
}

impl<'v> Freeze for Struct<'v> {
    type Frozen = FrozenStruct;
    fn freeze(self, freezer: &Freezer) -> anyhow::Result<Self::Frozen> {
        let mut frozen = SmallMap::with_capacity(self.fields.len());
        for (k, v) in self.fields.into_iter_hashed() {
            frozen.insert_hashed(k.freeze(freezer)?, v.freeze(freezer)?);
        }
        Ok(FrozenStruct {
            fields: frozen,
            _marker: marker::PhantomData,
        })
    }
}

impl<'v, V: ValueLike<'v>> StarlarkValue<'v> for StructGen<'v, V>
where
    Self: AnyLifetime<'v>,
{
    starlark_type!(Struct::TYPE);

    fn get_methods(&self) -> Option<&'static Globals> {
        static RES: GlobalsStatic = GlobalsStatic::new();
        RES.methods(crate::stdlib::structs::struct_methods)
    }

    fn extra_memory(&self) -> usize {
        self.fields.extra_memory()
    }

    fn to_json(&self) -> anyhow::Result<String> {
        let mut s = "{".to_owned();
        s += &self
            .fields
            .iter()
            .map(|(k, v)| {
                Ok(format!(
                    "\"{}\":{}",
                    k.to_string_value().as_str(),
                    v.to_json()?
                ))
            })
            .collect::<anyhow::Result<Vec<String>>>()?
            .join(",");
        s += "}";
        Ok(s)
    }

    fn equals(&self, other: Value<'v>) -> anyhow::Result<bool> {
        match Struct::from_value(other) {
            None => Ok(false),
            Some(other) => {
                equals_small_map(coerce_ref(&self.fields), &other.fields, |x, y| x.equals(*y))
            }
        }
    }

    fn compare(&self, other: Value<'v>) -> anyhow::Result<Ordering> {
        match Struct::from_value(other) {
            None => ValueError::unsupported_with(self, "cmp()", other),
            Some(other) => compare_small_map(
                coerce_ref(&self.fields),
                &other.fields,
                |k| k.to_string_value().as_str(),
                |x, y| x.compare(*y),
            ),
        }
    }

    fn get_attr(&self, attribute: &str, _heap: &'v Heap) -> Option<Value<'v>> {
        coerce_ref(&self.fields).get(attribute).copied()
    }

    fn write_hash(&self, hasher: &mut StarlarkHasher) -> anyhow::Result<()> {
        for (k, v) in self.fields.iter_hashed() {
            Hash::hash(&k, hasher);
            v.write_hash(hasher)?;
        }
        Ok(())
    }

    fn has_attr(&self, attribute: &str) -> bool {
        coerce_ref(&self.fields).contains_key(attribute)
    }

    fn dir_attr(&self) -> Vec<String> {
        self.fields
            .keys()
            .map(|x| x.to_string_value().as_str().to_owned())
            .collect()
    }
}

impl<'v> UnpackValue<'v> for &'v Struct<'v> {
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        Struct::from_value(value)
    }
}

/// Like [`ValueOf`](crate::values::ValueOf), but only validates value types; does not construct
/// or store a map.
pub struct StructOf<'v, V: UnpackValue<'v>> {
    value: ValueOf<'v, &'v Struct<'v>>,
    _marker: PhantomData<V>,
}

impl<'v, V: UnpackValue<'v>> UnpackValue<'v> for StructOf<'v, V> {
    fn unpack_value(value: Value<'v>) -> Option<StructOf<'v, V>> {
        let value = ValueOf::<&Struct>::unpack_value(value)?;
        for (_k, &v) in &value.typed.fields {
            // Validate field types
            V::unpack_value(v)?;
        }
        Some(StructOf {
            value,
            _marker: marker::PhantomData,
        })
    }
}

impl<'v, V: UnpackValue<'v>> StructOf<'v, V> {
    pub fn to_value(&self) -> Value<'v> {
        self.value.value
    }

    pub fn as_struct(&self) -> &Struct<'v> {
        self.value.typed
    }

    /// Collect field structs.
    pub fn to_map(&self) -> SmallMap<StringValue<'v>, V> {
        self.as_struct()
            .fields
            .iter()
            .map(|(&k, &v)| (k, V::unpack_value(v).expect("validated at construction")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::assert;

    #[test]
    fn test_to_json() {
        assert::pass(
            r#"
struct(key = None).to_json() == '{"key":null}'
struct(key = True).to_json() == '{"key":true}'
struct(key = False).to_json() == '{"key":false}'
struct(key = 42).to_json() == '{"key":42}'
struct(key = 'value').to_json() == '{"key":"value"}'
struct(key = 'value"').to_json() == '{"key":"value\\\""}'
struct(key = 'value\\').to_json() == '{"key":"value\\\\"}'
struct(key = 'value/').to_json() == '{"key":"value/"}'
struct(key = 'value\u0008').to_json() == '{"key":"value\\b"}'
struct(key = 'value\u000C').to_json() == '{"key":"value\\f"}'
struct(key = 'value\\n').to_json() == '{"key":"value\\n"}'
struct(key = 'value\\r').to_json() == '{"key":"value\\r"}'
struct(key = 'value\\t').to_json() == '{"key":"value\\t"}'
struct(foo = 42, bar = "some").to_json() == '{"foo":42,"bar":"some"}'
struct(foo = struct(bar = "some")).to_json() == '{"foo":{"bar":"some"}}'
struct(foo = ["bar/", "some"]).to_json() == '{"foo":["bar/","some"]}'
struct(foo = [struct(bar = "some")]).to_json() == '{"foo":[{"bar":"some"}]}'
"#,
        );
    }
}
