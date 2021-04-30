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
use crate::user;

pub struct Scan<
    'a,
    E: user::EntryT<K, V, Cid, Umeta>,
    K: user::Hash,
    V: user::Val,
    Cid: user::Cid,
    Umeta: user::Meta<V>,
> {
    last: Option<::std::ptr::NonNull<E>>,
    f: &'a dyn Fn(::std::ptr::NonNull<E>) -> (),
    _k: ::std::marker::PhantomData<K>,
    _v: ::std::marker::PhantomData<V>,
    _cid: ::std::marker::PhantomData<Cid>,
    _umeta: ::std::marker::PhantomData<Umeta>,
}

impl<
        'a,
        E: user::EntryT<K, V, Cid, Umeta>,
        K: user::Hash,
        V: user::Val,
        Cid: user::Cid,
        Umeta: user::Meta<V>,
    > Scan<'a, E, K, V, Cid, Umeta>
{
    pub fn new(f: &'a dyn Fn(::std::ptr::NonNull<E>) -> ()) -> Self {
        Scan {
            last: None,
            f: f,
            _k: ::std::marker::PhantomData,
            _v: ::std::marker::PhantomData,
            _cid: ::std::marker::PhantomData,
            _umeta: ::std::marker::PhantomData,
        }
    }
    pub fn is_running(&self) -> bool {
        self.last != None
    }
    pub fn start_scan(&mut self, entry: ::std::ptr::NonNull<E>) {
        (self.f)(entry);
        self.last = Some(entry);
    }
    pub fn stop(&mut self) {
        self.last = None;
    }
    pub fn apply_raw(&self, entry: ::std::ptr::NonNull<E>) {
        (self.f)(entry);
    }
    /// Apply "f" to the entry in the tail, update the last node
    pub fn apply_next(&mut self) {
        if self.last == None {
            return;
        }
        let next_tail = unsafe { self.last.unwrap().as_mut().get_tail_ptr() };
        match next_tail {
            None => {
                self.last = None;
            }
            Some(next) => {
                (self.f)(next);
                self.last = Some(next);
            }
        }
    }
    /// When a node is removed, check if it was the "last" node we scanned.
    /// In that case we will have to advance to the tail ptr and re-apply "f"
    pub fn check_and_next(&mut self, entry: ::std::ptr::NonNull<E>) {
        match self.last {
            None => {}
            Some(mut ptr_e) => {
                if ptr_e == entry {
                    match unsafe { ptr_e.as_mut().get_tail_ptr() } {
                        None => {
                            self.last = None;
                        }
                        Some(ptr_next) => {
                            (self.f)(ptr_next);
                            self.last = Some(ptr_next);
                        }
                    }
                }
            }
        }
    }
}

pub fn null_scan<E, K, V, Cid, Umeta>(_entry: ::std::ptr::NonNull<E>)
where
    E: crate::user::EntryT<K, V, Cid, Umeta>,
    K: user::Hash,
    V: user::Val,
    Cid: user::Cid,
    Umeta: user::Meta<V>,
{
    // do nothing
}
