#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalarType {
    Bool,
    Char,
    SignedChar,
    UnsignedChar,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Enum,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    Pointer,
    Float,
    Double,
    LongDouble,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordKind {
    Struct,
    Union,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

impl Field {
    pub fn new(name: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Scalar(ScalarType),
    Array(Box<Type>, usize),
    Record(RecordKind, Vec<Field>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldLayout {
    pub name: String,
    pub offset: usize,
    pub layout: TypeLayout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeLayout {
    pub size: usize,
    pub alignment: usize,
    pub fields: Vec<FieldLayout>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    ZeroLengthArray,
    EmptyRecord,
    SizeOverflow,
}

pub const PLAIN_CHAR_IS_UNSIGNED: bool = true;
pub const STACK_ALIGNMENT: usize = 16;

pub fn layout_of(ty: &Type) -> Result<TypeLayout, LayoutError> {
    match ty {
        Type::Scalar(scalar) => {
            let (size, alignment) = match scalar {
                ScalarType::Bool
                | ScalarType::Char
                | ScalarType::SignedChar
                | ScalarType::UnsignedChar => (1, 1),
                ScalarType::Short | ScalarType::UnsignedShort => (2, 2),
                ScalarType::Int | ScalarType::UnsignedInt | ScalarType::Enum => (4, 4),
                ScalarType::Long
                | ScalarType::UnsignedLong
                | ScalarType::LongLong
                | ScalarType::UnsignedLongLong
                | ScalarType::Pointer
                | ScalarType::Double => (8, 8),
                ScalarType::Float => (4, 4),
                ScalarType::LongDouble => (16, 16),
            };
            Ok(TypeLayout {
                size,
                alignment,
                fields: Vec::new(),
            })
        }
        Type::Array(element, count) => {
            if *count == 0 {
                return Err(LayoutError::ZeroLengthArray);
            }
            let element_layout = layout_of(element)?;
            let size = element_layout
                .size
                .checked_mul(*count)
                .ok_or(LayoutError::SizeOverflow)?;
            Ok(TypeLayout {
                size,
                alignment: element_layout.alignment,
                fields: Vec::new(),
            })
        }
        Type::Record(kind, fields) => layout_record(*kind, fields),
    }
}

fn layout_record(kind: RecordKind, fields: &[Field]) -> Result<TypeLayout, LayoutError> {
    if fields.is_empty() {
        return Err(LayoutError::EmptyRecord);
    }

    let mut field_layouts = Vec::with_capacity(fields.len());
    let mut size = 0usize;
    let mut alignment = 1usize;

    for field in fields {
        let layout = layout_of(&field.ty)?;
        alignment = alignment.max(layout.alignment);
        let offset = match kind {
            RecordKind::Struct => {
                size = align_up(size, layout.alignment)?;
                let offset = size;
                size = size
                    .checked_add(layout.size)
                    .ok_or(LayoutError::SizeOverflow)?;
                offset
            }
            RecordKind::Union => {
                size = size.max(layout.size);
                0
            }
        };
        field_layouts.push(FieldLayout {
            name: field.name.clone(),
            offset,
            layout,
        });
    }

    size = align_up(size, alignment)?;
    Ok(TypeLayout {
        size,
        alignment,
        fields: field_layouts,
    })
}

fn align_up(value: usize, alignment: usize) -> Result<usize, LayoutError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(LayoutError::SizeOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(ty: ScalarType) -> Type {
        Type::Scalar(ty)
    }

    #[test]
    fn scalar_layout_matches_lp64d_data_model() {
        let cases = [
            (ScalarType::Bool, 1, 1),
            (ScalarType::Char, 1, 1),
            (ScalarType::SignedChar, 1, 1),
            (ScalarType::UnsignedChar, 1, 1),
            (ScalarType::Short, 2, 2),
            (ScalarType::UnsignedShort, 2, 2),
            (ScalarType::Int, 4, 4),
            (ScalarType::UnsignedInt, 4, 4),
            (ScalarType::Enum, 4, 4),
            (ScalarType::Long, 8, 8),
            (ScalarType::UnsignedLong, 8, 8),
            (ScalarType::LongLong, 8, 8),
            (ScalarType::UnsignedLongLong, 8, 8),
            (ScalarType::Pointer, 8, 8),
            (ScalarType::Float, 4, 4),
            (ScalarType::Double, 8, 8),
            (ScalarType::LongDouble, 16, 16),
        ];
        for (ty, size, alignment) in cases {
            let layout = layout_of(&scalar(ty)).unwrap();
            assert_eq!((layout.size, layout.alignment), (size, alignment), "{ty:?}");
        }
        assert!(PLAIN_CHAR_IS_UNSIGNED);
        assert_eq!(STACK_ALIGNMENT, 16);
    }

    #[test]
    fn structure_layout_applies_member_alignment_and_tail_padding() {
        let ty = Type::Record(
            RecordKind::Struct,
            vec![
                Field::new("tag", scalar(ScalarType::Char)),
                Field::new("value", scalar(ScalarType::Int)),
                Field::new("ratio", scalar(ScalarType::Double)),
            ],
        );
        let layout = layout_of(&ty).unwrap();
        assert_eq!((layout.size, layout.alignment), (16, 8));
        assert_eq!(
            layout
                .fields
                .iter()
                .map(|field| (field.name.as_str(), field.offset))
                .collect::<Vec<_>>(),
            [("tag", 0), ("value", 4), ("ratio", 8)]
        );
    }

    #[test]
    fn nested_array_and_binary128_layout_is_explicit() {
        let ty = Type::Record(
            RecordKind::Struct,
            vec![
                Field::new("tag", scalar(ScalarType::Char)),
                Field::new("extended", scalar(ScalarType::LongDouble)),
                Field::new(
                    "samples",
                    Type::Array(Box::new(scalar(ScalarType::Short)), 3),
                ),
            ],
        );
        let layout = layout_of(&ty).unwrap();
        assert_eq!((layout.size, layout.alignment), (48, 16));
        assert_eq!(
            layout
                .fields
                .iter()
                .map(|field| field.offset)
                .collect::<Vec<_>>(),
            [0, 16, 32]
        );
        assert_eq!(layout.fields[2].layout.size, 6);
    }

    #[test]
    fn union_members_share_storage_and_union_size_is_padded() {
        let ty = Type::Record(
            RecordKind::Union,
            vec![
                Field::new("integer", scalar(ScalarType::Int)),
                Field::new("extended", scalar(ScalarType::LongDouble)),
            ],
        );
        let layout = layout_of(&ty).unwrap();
        assert_eq!((layout.size, layout.alignment), (16, 16));
        assert!(layout.fields.iter().all(|field| field.offset == 0));
    }

    #[test]
    fn invalid_or_overflowing_object_layouts_fail_explicitly() {
        assert_eq!(
            layout_of(&Type::Array(Box::new(scalar(ScalarType::LongDouble)), 0)),
            Err(LayoutError::ZeroLengthArray)
        );
        assert_eq!(
            layout_of(&Type::Record(RecordKind::Struct, vec![])),
            Err(LayoutError::EmptyRecord)
        );
        assert_eq!(
            layout_of(&Type::Array(
                Box::new(scalar(ScalarType::LongDouble)),
                usize::MAX
            )),
            Err(LayoutError::SizeOverflow)
        );
    }
}
