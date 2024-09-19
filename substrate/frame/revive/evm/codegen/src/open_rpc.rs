//! Defines the types defined by the [`OpenRPC`](https://spec.open-rpc.org) specification.

#![warn(missing_docs, missing_debug_implementations)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Represents an OpenRPC document.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpenRpc {
    /// The semantic version number of the OpenRPC Specification version that the OpenRPC document
    /// uses.
    ///
    /// This field should be used by tooling specifications and clients to interpret the OpenRPC
    /// document.
    pub openrpc: String,
    /// Provides metadata about the API.
    ///
    /// This metadata may be used by tooling as required.
    pub info: Info,
    /// An array of [`Server`] objects, which provide connectivity information to a target server.
    ///
    /// If the `servers` property is not provided, or is an empty array, the default value would
    /// be a [`Server`] with a `url` value of `localhost`. This is taken care of by the
    /// [`open-rpc`](crate) crate.
    #[serde(default = "serde_fns::servers")]
    pub servers: Vec<Server>,
    /// The available methods for the API. While this field is required, it is legal to leave it
    /// empty.
    pub methods: Vec<RefOr<Method>>,
    /// Holds various schemas for the specification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    /// Contains additional documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
}

impl OpenRpc {
    /// Returns the [`Method`] with the given path reference.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let path = "#/components/schemas/MY_SCHEMA";
    /// let schema = openrpc.get_schema(path).unwrap();
    /// ```
    pub fn get_schema(&self, reference: &str) -> Option<&Schema> {
        let mut components = reference.split('/');

        if !matches!(components.next(), Some("#")) {
            return None;
        }

        if !matches!(components.next(), Some("components")) {
            return None;
        }

        if !matches!(components.next(), Some("schemas")) {
            return None;
        }

        let name = components.next()?;
        self.components.as_ref()?.schemas.get(name)
    }

    /// Same as [`OpenRpc::get_schema`] but returns a &mut reference
    pub fn get_schema_mut(&mut self, reference: &str) -> Option<&mut Schema> {
        let mut components = reference.split('/');

        if !matches!(components.next(), Some("#")) {
            return None;
        }

        if !matches!(components.next(), Some("components")) {
            return None;
        }

        if !matches!(components.next(), Some("schemas")) {
            return None;
        }

        let name = components.next()?;
        self.components.as_mut()?.schemas.get_mut(name)
    }
}

/// Provides metadata about the API.
///
/// The metadata may be used by clients if needed, and may be presented in editing or
/// documentation generation tools for convenience.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// The title of the application.
    #[serde(default)]
    pub title: String,
    /// A verbose description of the application.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A URL to the Terms of Service for the API.
    ///
    /// This must contain an URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms_of_service: Option<String>,
    /// contact information for the exposed API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
    /// License information for the exposed API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    /// The version of the OpenRPC document.
    ///
    /// Note that this is distinct from the `openrpc` field of [`OpenRpc`] which specifies the
    /// version of the OpenRPC Specification used.
    #[serde(default)]
    pub version: String,
}

/// Contact information for the exposed API.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    /// The identifying name of the contact person/organization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The URL pointing to the contact information.
    ///
    /// This must contain an URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The email address of the contact person/organization.
    ///
    /// This must contain an email address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// License information for the exposed API.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct License {
    /// The name of the license used for the API.
    #[serde(default)]
    pub name: String,
    /// The URL pointing to the license used for the API.
    ///
    /// This must contain an URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A server.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    /// A name to be used as the canonical name for the server.
    #[serde(default)]
    pub name: String,
    /// A URL to the target host.
    ///
    /// This URL supports Server Variables and may be relative to indicate that the host location
    /// is relative to the location where the OpenRPC document is being served.
    ///
    /// Server Variables are passed into the Runtime Expression to produce a server URL.
    pub url: RuntimeExpression,
    /// A short description of what the server is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Describes the host designated by the URL.
    ///
    /// GitHub Flavored Markdown may be used for rich text presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The values of this object are passed to the [`RuntimeExpression`] to produce an actual
    /// URL.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, ServerVariable>,
}

/// An object representing a Server Variable for server URL template substitution.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerVariable {
    /// An enumeration of string values to be used if the substitution options are from a limited
    /// set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_: Vec<String>,
    /// The default value to use for substitution, which shall be sent if an alternate value is
    /// not supplied.
    ///
    /// Note this behavior is different than the Schema Object's treatment of default values,
    /// because in those cases parameter values are optional.
    #[serde(default)]
    pub default: String,
    /// An optional description for the server variable.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Describes the interface for the given method name.
///
/// The method name is used as the `method` field of the JSON-RPC body. It therefore must be
/// unique.
#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    /// The canonical name of the method.
    ///
    /// This name must be unique within the methods array.
    #[serde(default)]
    pub name: String,
    /// A list of tags for API documentation control. Tags can be used for logical grouping
    /// of methods by resources or any other qualifier.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<RefOr<Tag>>,
    /// A short summary of what the method does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A verbose explanation of the method behavior.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional external documentation for this method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
    /// A list of parameters that are applicable for this method.
    ///
    /// The list must not include duplicated parameters and therefore require `name` to be
    /// unique.
    ///
    /// All required parameters must be listed *before* any optional parameters.
    #[serde(default)]
    pub params: Vec<RefOr<ContentDescriptor>>,
    /// The description of the result returned by the method.
    ///
    /// If defined, it must be a [`ContentDescriptor`] or a Reference.
    ///
    /// If undefined, the method must only be used as a *notification*.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RefOr<ContentDescriptor>>,
    /// Declares this method as deprecated.
    ///
    /// Consumers should refrain from usage of the declared method.
    ///
    /// The default value is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub deprecated: bool,
    /// An alternative `servers` array to service this method.
    ///
    /// If specified, it overrides the `servers` array defined at the root level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    /// A list of custom application-defined errors that may be returned.
    ///
    /// The errors must have unique error codes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<RefOr<Error>>,
    /// A list of possible links from this method call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<RefOr<Link>>,
    /// The expected format of the parameters.
    ///
    /// The parameters of a method may be an array, an object, or either. When a method
    /// has a `param_structure` value of [`ByName`], callers of the method must pass an
    /// object as the parameters. When a method has a `param_structure` value of [`ByPosition`],
    /// callers of the method must pass an array as the parameters. Otherwise, callers may
    /// pass either an array or an object as the parameters.
    ///
    /// The default value is [`Either`].
    ///
    /// [`ByName`]: ParamStructure::ByName
    /// [`ByPosition`]: ParamStructure::ByPosition
    /// [`Either`]: ParamStructure::Either
    #[serde(default, skip_serializing_if = "serde_fns::is_default")]
    pub param_structure: ParamStructure,
    /// An array of [`ExamplePairing`] objects, where each example includes a valid
    /// params-to-result [`ContentDescriptor`] pairing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<RefOr<ExamplePairing>>,
}

/// A possible value for the `param_structure` field of [`Method`].
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ParamStructure {
    /// Parameters must be passed as a JSON object.
    ByName,
    /// Parameters must be passed as a JSON array.
    ByPosition,
    /// Parameters may be passed as either a JSON object or a JSON array.
    #[default]
    Either,
}

/// Content descriptors are that do just as they suggest - describe content. They are reusable
/// ways of describing either parameters or results.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContentDescriptor {
    /// The name of the content being described.
    ///
    /// If the content described is a method parameter assignable
    /// [`ByName`](ParamStructure::ByName), this field must be the name of the parameter.
    #[serde(default)]
    pub name: String,
    /// A short summary of the content that is being described.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A verbose explanation of the content being described.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Determines if the content is a required field.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub required: bool,
    /// A [`Schema`] that describes what is allowed in the content.
    #[serde(default)]
    pub schema: Schema,
    /// Whether the content is deprecated.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub deprecated: bool,
}

/// Allows the definition of input and output data types.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    /// The title of the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The description of the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The contents of the schema.
    #[serde(flatten)]
    pub contents: SchemaContents,
}

/// The content of a schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SchemaContents {
    /// The schema contains a reference to another schema.
    Reference {
        /// The reference string.
        #[serde(rename = "$ref")]
        reference: String,
    },
    /// The schema is made of a combination of other schemas.
    ///
    /// The final object must match *all* of the schemas.
    AllOf {
        /// The schemas that the final object must match.
        #[serde(rename = "allOf")]
        all_of: Vec<Schema>,
    },
    /// The schema is made of a combination of other schemas.
    ///
    /// The final object must match *any* of the schemas.
    AnyOf {
        /// The schemas that the final object must match.
        #[serde(rename = "anyOf")]
        any_of: Vec<Schema>,
    },
    /// The schema is made of a combination of other schemas.
    ///
    /// The final object must match exactly *one* of the schemas.
    OneOf {
        /// The schemas that the final object must match.
        #[serde(rename = "oneOf")]
        one_of: Vec<Schema>,
    },
    /// The schema contains a literal value.
    Literal(Literal),
    /// The schema contains an Object.
    ///
    /// Note this is a workaround to parse Literal(Literal::ObjectLiteral), that don't havethe
    /// type: "object" field.
    Object(ObjectLiteral),
}

impl Default for SchemaContents {
    #[inline]
    fn default() -> Self {
        Self::Literal(Literal::Null)
    }
}

/// A literal value.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Literal {
    /// The literal is a boolean.
    Boolean,
    /// The literal is an integer.
    Integer(IntegerLiteral),
    /// The literal is a number.
    Number(NumberLiteral),
    /// The literal is a string.
    String(StringLiteral),
    // The literal is an object.
    Object(ObjectLiteral),
    /// The literal is an array.
    Array(ArrayLiteral),
    /// The literal is a null value.
    Null,
}

/// The constraints that may be applied to an integer literal schema.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone)]
pub struct IntegerLiteral {
    /// The integer must be a multiple of this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<i64>,
    /// The minimum value of the integer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i64>,
    /// The maximum value of the integer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<i64>,
    /// Whether the minimum value is exclusive.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub exclusive_minimum: bool,
    /// Whether the maximum value is exclusive.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub exclusive_maximum: bool,
}

/// The constraints that may be applied to a number literal schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NumberLiteral {
    /// The number must be a multiple of this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f64>,
    /// The minimum value of the number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// The maximum value of the number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// Whether the minimum value is exclusive.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub exclusive_minimum: bool,
    /// Whether the maximum value is exclusive.
    ///
    /// Default is `false`.
    #[serde(default, skip_serializing_if = "serde_fns::is_false")]
    pub exclusive_maximum: bool,
}

/// The constraints that may be applied to an array literal schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArrayLiteral {
    /// The schema that the items in the array must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
}

/// The constraints that may be applied to an string literal schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StringLiteral {
    /// The minimum length of the string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,
    /// The maximum length of the string.s
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    /// The pattern that the string must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// The format that the string must be in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<StringFormat>,
    /// A list of possible values for the string.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "enum")]
    pub enumeration: Option<Vec<String>>,
}

/// A string format.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum StringFormat {
    /// Date and time together, for example, `2018-11-13T20:20:39+00:00`.
    DateTime,
    /// Time, for example, `20:20:39+00:00`.
    Time,
    /// Date, for example, `2018-11-13`.
    Date,
    /// A duration as defined by the [ISO 8601 ABNF](https://datatracker.ietf.org/doc/html/rfc3339#appendix-A).
    Duration,
    /// An email. See [RFC 5321](http://tools.ietf.org/html/rfc5321#section-4.1.2).
    Email,
    /// The internationalized version of an email. See [RFC 6531](https://tools.ietf.org/html/rfc6531).
    IdnEmail,
    /// A host name. See [RFC 1123](https://datatracker.ietf.org/doc/html/rfc1123#section-2.1).
    Hostname,
    /// The internationalized version of a host name. See [RFC 5890](https://tools.ietf.org/html/rfc5890#section-2.3.2.3).
    IdnHostname,
    /// An IP v4. See [RFC 2673](http://tools.ietf.org/html/rfc2673#section-3.2).
    #[serde(rename = "ipv4")]
    IpV4,
    /// An IP v6. See [RFC 2373](http://tools.ietf.org/html/rfc2373#section-2.2).
    #[serde(rename = "ipv6")]
    IpV6,
    /// A universally unique identifier. See [RFC 4122](https://datatracker.ietf.org/doc/html/rfc4122).
    Uuid,
    /// A universal resource identifier . See [RFC 3986](http://tools.ietf.org/html/rfc3986).
    Uri,
    /// A URI reference. See (RFC 3986)[<http://tools.ietf.org/html/rfc3986#section-4.1>].
    UriReference,
    /// The internationalized version of a URI. See [RFC 3987](https://tools.ietf.org/html/rfc3987).
    Iri,
    /// The internationalized version of a URI reference. See [RFC 3987](https://tools.ietf.org/html/rfc3987).
    IriReference,
    /// A URI template. See [RFC 6570](https://tools.ietf.org/html/rfc6570).
    UriTemplate,
    /// A JSON pointer. See [RFC 6901](https://tools.ietf.org/html/rfc6901).
    JsonPointer,
    /// A relative JSON pointer. See [Relative JSON Pointer](https://tools.ietf.org/html/draft-handrews-relative-json-pointer-01).
    RelativeJsonPointer,
    /// A regular expression. See [ECMA 262](https://www.ecma-international.org/publications-and-standards/standards/ecma-262/).
    Regex,
}

/// The constraints that may be applied to an object literal schema.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ObjectLiteral {
    /// The properties that the object might have.
    pub properties: BTreeMap<String, Schema>,

    /// List of legacy aliases for properties.
    #[serde(skip)]
    pub legacy_aliases: HashMap<String, String>,

    /// A list of properties that the object must have.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

/// A set of example parameters and a result.
///
/// This result is what you'd expect from the JSON-RPC service given the exact params.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExamplePairing {
    /// The name for the example pairing.
    #[serde(default)]
    pub name: String,
    /// A verbose description of the example pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A short summary of the example pairing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Example parameters.
    #[serde(default)]
    pub params: Vec<RefOr<ExampleObject>>,
    /// Example result.
    ///
    /// When undefined, shows the usage of the method as a notification.
    #[serde(default)]
    pub result: RefOr<ExampleObject>,
}

/// Defines an example that is intended to match a [`Schema`] of a given [`ContentDescriptor`].
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExampleObject {
    /// Canonical name of the example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A verbose description of the example
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A short summary of the example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The value of the example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ExampleValue>,
}

/// The example value of an [`ExampleObject`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ExampleValue {
    /// The value is a JSON object embedded in the document.
    /// A link to an external document containing the value.
    #[serde(rename = "externalValue")]
    External(String),
}

/// Represents a possible design-time link for a result.
///
/// The presence of a link does not guarantee the caller's ability to successfully invoke it,
/// rather it provides a known relationship and traversal mechanism between results and other
/// methods.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug, Clone)]
pub struct Link {
    /// Canonical name for the link.
    #[serde(default)]
    pub name: String,
    /// A description of the link.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Short description for the link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The name of an *existing*, resolvable OpenRPC method, as defined with a unique
    /// `method`. This field must resolve to a unique [`Method`] object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The parameters to pass to a method as specified with `method`. The key is the parameter
    /// name to be used, whereas the value can be a constant or a [`RuntimeExpression`] to be
    /// evaluated and passed to the linked method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<LinkParams>,
    /// A server object to be used by the target method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Server>,
}

/// The content of the `params` field of a [`Link`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum LinkParams {
    /// A [`RuntimeExpression`] that evaluates to the parameters.
    Dynamic(RuntimeExpression),
}

/// Runtime expressions allow the user to define an expression which will evaluate to a
/// string once the desired value(s) are known.
///
/// They are used when the desired value of a link or server can only be constructed at
/// run time. This mechanism is used by [`Link`] objects and [`ServerVariable`]s.
///
/// This runtime expression makes use of JSON template strings.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(transparent)]
pub struct RuntimeExpression(pub String);

/// An application-level error.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// An application-defined error code.
    #[serde(default)]
    pub code: i64,
    /// A string providing a short description of the error.
    ///
    /// The message should be limited to a concise single sentence.
    #[serde(default)]
    pub message: String,
}

/// Holds a set of reusable objects for different aspects of the OpenRPC document.
///
/// All objects defined within the [`Components`] object will have no effect on the API
/// unless they are explicitly referenced from properties outside of the [`Components`]
/// object.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Components {
    /// A list of reusable [`ContentDescriptor`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub content_descriptors: BTreeMap<String, ContentDescriptor>,
    /// A list of reusable [`Schema`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub schemas: BTreeMap<String, Schema>,
    /// A list of reusable [`ExampleObject`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub examples: BTreeMap<String, ExampleObject>,
    /// A list of reusable [`Link`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, Link>,
    /// A list of reusable [`Error`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, Error>,
    /// A list of reusable [`ExamplePairing`]s.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        rename = "examplePairingObjects"
    )]
    pub example_pairings: BTreeMap<String, ExamplePairing>,
    /// A list of reusable [`Tag`]s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, Tag>,
}

/// Adds metadata to a single tag that is used by the [`Method`] Object.
///
/// It is not mandatory to have a [`Tag`] Object per tag defined in the [`Method`]
/// Object instances.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// The name of the tag.
    #[serde(default)]
    pub name: String,
    /// A short summary of the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A verbose explanation of the tag.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional external documentation for this tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocumentation>,
}

/// Allows referencing an external resource for extended documentation.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocumentation {
    /// A verbose explanation of the target documentation.
    ///
    /// GitHub Flavored Markdown syntax may be used for rich text representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A URL for the target documentation.
    ///
    /// This must contain an URL.
    #[serde(default)]
    pub url: String,
}

/// Either a reference or an inline object.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum RefOr<T> {
    /// A reference to an object defined elsewhere.
    Reference {
        /// The reference string.
        #[serde(rename = "$ref")]
        reference: String,
    },
    /// An inline object.
    Inline(T),
}

impl<T> RefOr<T> {
    /// Unwraps the inlined object.
    pub fn unwrap_inline(&self) -> &T {
        match self {
            RefOr::Reference { reference } => panic!("Unexpected reference: {reference}"),
            RefOr::Inline(v) => v,
        }
    }
}

impl<T: Default> Default for RefOr<T> {
    #[inline]
    fn default() -> Self {
        RefOr::Inline(T::default())
    }
}

/// Functions used by `serde`, such as predicates and default values.
mod serde_fns {
    use std::collections::BTreeMap;

    use super::{RuntimeExpression, Server};

    /// Returns the default value of the `servers` field.
    pub fn servers() -> Vec<Server> {
        vec![Server {
            name: "default".into(),
            url: RuntimeExpression("localhost".into()),
            summary: None,
            description: None,
            variables: BTreeMap::new(),
        }]
    }

    /// Returns whether `b` is `false`.
    pub fn is_false(b: &bool) -> bool {
        !*b
    }

    /// Returns whether the given value is the default value of its type.
    pub fn is_default<T: Default + PartialEq>(t: &T) -> bool {
        *t == T::default()
    }
}

#[test]
fn parsing_works() {
    let content = include_str!("../openrpc.json");
    let _: OpenRpc = dbg!(serde_json::from_str(content).unwrap());
}
