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

use crate::values::dict::DictRef;
use crate::values::type_repr::DictType;
use crate::values::type_repr::StarlarkTypeRepr;
use crate::values::UnpackValue;
use crate::values::Value;

/// Unpack `dict`.
///
/// There's `impl` [`UnpackValue`] for [`SmallMap`](starlark_map::small_map::SmallMap)
/// but this can be used when hashing of unpacked keys is not needed.
pub struct UnpackDictEntries<K, V> {
    /// Entries of the dictionary.
    pub entries: Vec<(K, V)>,
}

impl<K: StarlarkTypeRepr, V: StarlarkTypeRepr> StarlarkTypeRepr for UnpackDictEntries<K, V> {
    type Canonical = <DictType<K, V> as StarlarkTypeRepr>::Canonical;

    fn starlark_type_repr() -> crate::typing::Ty {
        DictType::<K, V>::starlark_type_repr()
    }
}

impl<'v, K: UnpackValue<'v>, V: UnpackValue<'v>> UnpackValue<'v> for UnpackDictEntries<K, V> {
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        let dict = DictRef::unpack_value(value)?;
        let mut entries = Vec::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            entries.push((K::unpack_value(k)?, V::unpack_value(v)?));
        }
        Some(UnpackDictEntries { entries })
    }
}
