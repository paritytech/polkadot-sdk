use crate::open_rpc::*;
use inflector::Inflector;

/// Type information used for generating the type.
#[derive(Debug)]
pub struct TypeInfo {
    /// The type name.
    pub name: String,
    /// Whether the type is an array.
    pub array: bool,
    /// Whether the type is required.
    pub required: Required,
}

impl TypeInfo {
    pub fn set_required(mut self, required: bool) -> Self {
        if required {
            self.required = Required::Yes;
        } else {
            self.required = Required::No { skip_if_null: true };
        }
        self
    }

    /// Return Whether the type is optional.
    pub fn is_optional(&self) -> bool {
        matches!(self.required, Required::No { .. })
    }
}

/// A trait to provide type names.
pub trait TypeNameProvider {
    /// Returns  type information for a schema.
    fn type_info(&mut self, schema: &Schema) -> Option<TypeInfo>;
}

/// Describes whether the type is required or not.
#[derive(Debug)]
pub enum Required {
    /// The type is required.
    Yes,
    /// The type is not required, and may be skipped when serializing if it's None and skip_if_null
    /// is true.
    No { skip_if_null: bool },
}

impl TypeInfo {
    //// Convert the type info to a string we can use in the generated code.
    pub fn get_type(&self) -> String {
        let mut type_name = self.name.clone();
        if self.array {
            type_name = format!("Vec<{}>", type_name)
        }
        if self.is_optional() {
            type_name = format!("Option<{}>", type_name)
        }
        type_name
    }
}

impl<T> From<T> for TypeInfo
where
    T: Into<String>,
{
    fn from(name: T) -> Self {
        Self {
            name: name.into(),
            required: Required::Yes,
            array: false,
        }
    }
}
/// Represents a field in a struct.
#[derive(Debug)]
pub struct Field {
    /// The documentation for the field.
    doc: Option<String>,
    /// The name of the field.
    name: String,
    /// the type information for the field.
    type_info: TypeInfo,
    /// Whether to flatten the field, when serializing.
    flatten: bool,
    /// Legacy alias for the field.
    alias: Option<String>,
}

/// Represents a collection of fields.
#[derive(Debug)]
pub struct Fields(Vec<Field>);

impl From<Vec<Field>> for Fields {
    fn from(value: Vec<Field>) -> Self {
        Self(value)
    }
}

impl IntoIterator for Fields {
    type Item = Field;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Fields {
    /// Creates a collection of fields from an [`ObjectLiteral].
    ///
    /// The methods also takes a [`TypeNameProvider`] to resolve the types of the fields, and to
    /// collect child types.
    pub fn from(value: &ObjectLiteral, provider: &mut impl TypeNameProvider) -> Self {
        let ObjectLiteral {
            properties,
            legacy_aliases,
            required,
        } = value;

        properties
            .iter()
            .map(|(name, schema)| {
                let mut type_info = provider.type_info(schema).expect("Type should be defined");
                if matches!(type_info.required, Required::Yes) && !required.contains(name) {
                    type_info.required = Required::No { skip_if_null: true };
                }

                let doc = doc_str_from_schema(schema);
                Field {
                    doc,
                    name: name.clone(),
                    type_info,
                    alias: legacy_aliases.get(name).cloned(),
                    flatten: false,
                }
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// Creates a collection of fields from the items of a [`SchemaContents::AllOf`] schema.
    pub fn from_all_of(all_of: &[Schema], provider: &mut impl TypeNameProvider) -> Fields {
        all_of
            .iter()
            .flat_map(|schema| {
                let doc = doc_str_from_schema(schema);
                if let Some(type_info) = provider.type_info(schema) {
                    vec![Field {
                        doc,
                        name: type_info.name.clone(),
                        type_info,
                        alias: None,
                        flatten: true,
                    }]
                } else {
                    let object = match &schema.contents {
                        SchemaContents::Object(object) => object,
                        SchemaContents::Literal(Literal::Object(object)) => object,
                        v => panic!("Unsupported anonymous all_of type {:?}", v),
                    };

                    Fields::from(object, provider).0
                }
            })
            .collect::<Vec<_>>()
            .into()
    }
}

/// The variant of an enum.
#[derive(Debug)]
pub struct Variant {
    /// The documentation for the variant.
    doc: Option<String>,
    /// The type information for the variant.
    type_info: TypeInfo,
}

impl Variant {
    pub fn name(&self) -> String {
        if self.type_info.array {
            format!("{}s", self.type_info.name)
        } else {
            self.type_info.name.clone()
        }
    }
}

pub fn doc_str_from_schema(schema: &Schema) -> Option<String> {
    let mut doc = schema.title.clone();

    if let Some(description) = &schema.description {
        doc = Some(doc.map_or_else(
            || description.clone(),
            |doc| format!("{doc}\n{description}"),
        ));
    }

    doc
}

#[derive(Debug)]
pub struct Variants(Vec<Variant>);
impl Variants {
    /// Creates a collection of variants from the items of a [`SchemaContents::OneOf`] schema.
    pub(crate) fn from_one_of(one_of: &[Schema], provider: &mut impl TypeNameProvider) -> Variants {
        one_of
            .iter()
            .filter_map(|schema| {
                let doc = doc_str_from_schema(schema);
                let type_info = provider.type_info(schema).expect("Type should be defined");
                if type_info.name == "Null" || type_info.name == "NotFound" {
                    return None;
                }

                Some(Variant { doc, type_info })
            })
            .collect::<Vec<_>>()
            .into()
    }
}

impl From<Vec<Variant>> for Variants {
    fn from(value: Vec<Variant>) -> Self {
        Self(value)
    }
}

/// The content of a type.
#[derive(Debug)]
pub enum TypeContent {
    /// A struct type.
    Struct(Fields),
    /// A unit struct type.
    TypeAlias(TypeInfo),
    /// An enum type.
    Enum(Variants),
    /// A serde untagged enum type.
    UntaggedEnum(Vec<String>),
}

/// A type printer.
#[derive(Debug)]
pub struct TypePrinter {
    pub doc: Option<String>,
    pub name: String,
    pub content: TypeContent,
}

/// A macro to write a formatted line to a buffer.
#[macro_export]
macro_rules! writeln {
    (@doc $s: ident, $doc: ident) => {
      $crate::writeln!(@doc $s, $doc, 0)
    };
    (@doc $s: ident, $doc: ident, $indent: literal) => {
        if let Some(doc) = $doc {
            for line in doc.lines() {
                writeln!($s, "{:indent$}/// {}", "", line, indent = $indent);
            }
        }
    };
    ($s: ident, $($arg: tt)*) => {
        $s.push_str(&format!($($arg)*));
        $s.push_str("\n");
    };



}

impl TypePrinter {
    /// Prints the type to a buffer.
    pub fn print(self, buffer: &mut String) {
        let Self {
            doc, name, content, ..
        } = self;

        writeln!(@doc buffer, doc);
        match content {
            TypeContent::Enum(variants) if variants.0.len() == 1 => {
                let type_info = &variants.0[0].type_info;
                writeln!(buffer, "pub type {name} = {};", type_info.get_type());
            }
            TypeContent::TypeAlias(type_info) => {
                writeln!(buffer, "pub type {name} = {};", type_info.get_type());
            }
            TypeContent::Enum(variants) => {
                writeln!(
                    buffer,
                    "#[derive(Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]"
                );
                writeln!(buffer, "#[serde(untagged)]");
                writeln!(buffer, "pub enum {name} {{");
                for variant in variants.0.iter() {
                    let doc = &variant.doc;
                    writeln!(@doc buffer, doc, 2);
                    writeln!(
                        buffer,
                        "  {}({}),",
                        variant.name(),
                        variant.type_info.get_type()
                    );
                }
                writeln!(buffer, "}}");

                // Implement Default trait
                let variant = variants.0[0].name();
                writeln!(buffer, "impl Default for {name} {{");
                writeln!(buffer, "  fn default() -> Self {{");
                writeln!(buffer, "    {name}::{variant}(Default::default())");
                writeln!(buffer, "  }}");
                writeln!(buffer, "}}");
            }
            TypeContent::UntaggedEnum(variants) => {
                writeln!(
                    buffer,
                    "#[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]"
                );
                writeln!(buffer, "pub enum {name} {{");
                for (i, name) in variants.iter().enumerate() {
                    writeln!(buffer, "  #[serde(rename = \"{name}\")]");
                    if i == 0 {
                        writeln!(buffer, "  #[default]");
                    }
                    let pascal_name = name.to_pascal_case();
                    writeln!(buffer, "  {pascal_name},");
                }
                writeln!(buffer, "}}");
            }
            TypeContent::Struct(fields) => {
                writeln!(
                    buffer,
                    "#[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]"
                );

                writeln!(buffer, "pub struct {name} {{");
                for Field {
                    doc,
                    name,
                    type_info,
                    alias,
                    flatten,
                } in fields
                {
                    writeln!(@doc buffer, doc, 2);
                    let mut snake_name = name.to_snake_case();
                    let mut serde_params = vec![];

                    if flatten {
                        serde_params.push("flatten".to_string());
                    } else if snake_name != name {
                        serde_params.push(format!("rename = \"{}\"", name));
                    }

                    if let Some(alias) = alias {
                        serde_params.push(format!("alias = \"{}\"", alias));
                    }

                    if matches!(type_info.required, Required::No { skip_if_null: true }) {
                        serde_params.push("skip_serializing_if = \"Option::is_none\"".to_string());
                    }

                    if !serde_params.is_empty() {
                        writeln!(buffer, "  #[serde({})]", serde_params.join(", "));
                    }

                    let type_name = type_info.get_type();

                    if snake_name == "type" {
                        snake_name = "r#type".to_string()
                    }
                    writeln!(buffer, "  pub {snake_name}: {type_name},");
                }
                writeln!(buffer, "}}");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    #[test]
    fn print_struct_works() {
        let gen = TypePrinter {
            doc: Some("A simple struct".to_string()),
            name: "SimpleStruct".to_string(),
            content: TypeContent::Struct(
                vec![
                    Field {
                        doc: Some("The first field".to_string()),
                        name: "firstField".to_string(),
                        type_info: "u32".into(),
                        flatten: false,
                        alias: None,
                    },
                    Field {
                        doc: None,
                        name: "second".to_string(),
                        type_info: TypeInfo {
                            name: "String".to_string(),
                            required: Required::No { skip_if_null: true },
                            array: false,
                        },
                        flatten: true,
                        alias: None,
                    },
                ]
                .into(),
            ),
        };
        let mut buffer = String::new();
        gen.print(&mut buffer);
        assert_eq!(
            buffer,
            indoc! {r#"
            /// A simple struct
            #[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]
            pub struct SimpleStruct {
              /// The first field
              #[serde(rename = "firstField")]
              pub first_field: u32,
              #[serde(flatten, skip_serializing_if = "Option::is_none")]
              pub second: Option<String>,
            }
            "#}
        );
    }

    #[test]
    fn print_untagged_enum_works() {
        let gen = TypePrinter {
            doc: Some("A simple untagged enum".to_string()),
            name: "SimpleUntaggedEnum".to_string(),
            content: TypeContent::UntaggedEnum(vec!["first".to_string(), "second".to_string()]),
        };
        let mut buffer = String::new();
        gen.print(&mut buffer);
        assert_eq!(
            buffer,
            indoc! {r#"
            /// A simple untagged enum
            #[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]
            pub enum SimpleUntaggedEnum {
              #[serde(rename = "first")]
              #[default]
              First,
              #[serde(rename = "second")]
              Second,
            }
            "#}
        );
    }

    #[test]
    fn print_enum_works() {
        let gen = TypePrinter {
            doc: Some("A simple enum".to_string()),
            name: "SimpleEnum".to_string(),
            content: TypeContent::Enum(
                vec![
                    Variant {
                        doc: Some("The Foo variant".to_string()),
                        type_info: "Foo".into(),
                    },
                    Variant {
                        doc: Some("The Bar variant".to_string()),
                        type_info: "Bar".into(),
                    },
                ]
                .into(),
            ),
        };
        let mut buffer = String::new();
        gen.print(&mut buffer);
        assert_eq!(
            buffer,
            indoc! {r#"
         /// A simple enum
         #[derive(Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
         #[serde(untagged)]
         pub enum SimpleEnum {
           /// The Foo variant
           Foo(Foo),
           /// The Bar variant
           Bar(Bar),
         }
         impl Default for SimpleEnum {
           fn default() -> Self {
             SimpleEnum::Foo(Default::default())
           }
         }
         "#}
        );
    }
}
