/*
 * Copyright 2021 Luca Fulchir <luker@fenrirproject.org>
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

pub mod lru;
pub mod results;
pub mod slru;

/// The trait UserMeta defines operations that will be run on certain operations
/// of the LRU
pub trait UserMeta<V> {
    /// create a new metadata struct with default values
    /// used if you don't want to specify one on insert(...)
    fn new() -> Self
    where
        Self: Sized;
    /// run every time the key is added or re-added
    /// as extra parameters you have:
    /// * old_meta: ref to the old metadata. used when you are re-adding the
    ///   same key, so that you can decide if you want to keep the old meta or
    ///   start anew
    /// * val: if somehow you need to modify the value every time we have an
    ///   access
    fn on_insert(&mut self, old_meta: Option<&Self>, val: Option<&mut V>);
    /// run every time the key is requested
    fn on_get(&mut self, val: &mut V);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
