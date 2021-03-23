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
