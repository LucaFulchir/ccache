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
#[derive(::thiserror::Error, Debug)]
pub enum Error {
    #[error("Key not found in lru")]
    KeyNotFound,
}

pub enum InsertResult<E> {
    OldEntry {
        clash: Option<E>,
        evicted: Option<E>,
    },
    OldTail {
        clash: Option<E>,
        evicted: E,
    },
    Success,
}
pub enum InsertResultShared<E> {
    OldEntry { evicted: Option<E> },
    OldTailPtr { evicted: ::std::ptr::NonNull<E> },
    Success,
}
