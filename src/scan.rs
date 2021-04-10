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

pub struct Scan<
    E,
    F: FnMut(::std::ptr::NonNull<E>) -> Option<::std::ptr::NonNull<E>>,
> {
    pub last: Option<::std::ptr::NonNull<E>>,
    pub f: F,
}

pub fn null_scan<E, K, V, Cid, Umeta>(
    _entry: ::std::ptr::NonNull<E>,
) -> Option<::std::ptr::NonNull<E>>
where
    E: crate::user::EntryT<K, V, Cid, Umeta>,
    Umeta: crate::user::Meta<V>,
{
    None
}
/*
impl<
        E: crate::user::EntryT<K, V, Cid, Umeta>,
        K,
        V,
        Cid,
        Umeta,
        F: FnMut(::std::ptr::NonNull<E>) -> Option<::std::ptr::NonNull<E>>,
    > Scan<E, K, V, Umeta, F>
{
    pub fn update_next(&mut self) {
        match self.last {
            None => {}
            Some(ptr_entry) => match ptr_entry.get().get_tail_ptr() {
                None => {
                    self.last = None;
                }
                Some(ptr_tail) => {
                    (self.f)(ptr_tail.into());
                    self.last = Some(ptr_tail.into());
                }
            },
        }
    }
}
*/
