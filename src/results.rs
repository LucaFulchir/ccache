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
pub enum InsertResultShared<E, K> {
    OldEntry(E),
    OldTail(E),
    OldTailKey(K), // used by the *Shared for moving instead of removing
    Success,
}
