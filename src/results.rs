#[derive(::thiserror::Error, Debug)]
pub enum Error {
    #[error("Key not found in lru")]
    KeyNotFound,
}

pub enum InsertResult<E> {
    OldEntry(E),
    OldTail(E),
    Success,
}
// FIXME: case missing:
// we can have both a clash (oldkey) and a tail eviction of different keys
// reason: take SLRU, the insert generates a clash with the protected lru,
// but the new entry always goes into the probation LRU, which can generate a
// OldTail
pub enum InsertResultShared<E, K> {
    OldEntry(E),
    OldTail(E),
    OldTailKey(K), // used by the *Shared for moving instead of removing
    Success,
}
