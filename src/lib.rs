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

//! A single-thread only library to implement multiple caches over the same
//! hashmap
//!
//! # Single thread
//! All of this is currently designed to be used only in single-thread
//! applications  
//! The project was born out of a need for an efficient cache for
//! a heavily sharded application, so everything is designed purely for
//! single-thread.  
//! Contributions are welcome for multithread support
//!
//! # Shared hashmap
//!
//! Any hashmap is usable as long as the corresponding trait is implemented
//! We have included a basic hashmap that lets you access each slot by its
//! index ! and is stable, so that insertion or removal does not reshuffle
//! elements
//!
//! # Composable caches
//! Each cache is usable on the same hashmap, so they must coordinate a bit
//! through its The project currently implements:
//! * [LRU](lru)
//! * [SLRU](slru)
//! * [Scan-W-TLFU](swtlfu), a W-TLFU variant

/// stable hashmap implementation, based on `hashbrown::raw::RawTable`
pub mod hashmap;
pub mod lru;
/// common result for insert/get operations
pub mod results;
// not public, wrapper to scan each entry
mod scan;
pub mod slru;
pub mod swtlfu;
