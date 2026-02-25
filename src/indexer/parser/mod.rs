#[allow(unused_imports)]
pub mod core;
pub mod chunking;
pub mod type_fields;
pub mod skeleton;

// Реэкспорт публичного API — потребители не меняются
#[allow(unused_imports)]
pub use self::core::{CodeParser, ParsedFile, ParserError, Result, SupportedLanguage};
#[allow(unused_imports)]
pub use chunking::smart_chunk_file;
#[allow(unused_imports)]
pub use type_fields::{normalize_field, parse_type_fields, parse_all_type_fields};
#[allow(unused_imports)]
pub use skeleton::generate_skeleton;
