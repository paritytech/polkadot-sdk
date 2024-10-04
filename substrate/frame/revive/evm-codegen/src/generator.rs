use indoc::indoc;
use inflector::Inflector;
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	mem,
};

use crate::{
	open_rpc::*,
	printer::{
		doc_str_from_schema, Fields, Required, TypeContent, TypeInfo, TypeNameProvider,
		TypePrinter, Variants,
	},
	writeln,
};

lazy_static! {
  /// List of supported Ethereum RPC methods we want to generate.
  static ref SUPPORTED_ETH_METHODS: Vec<&'static str> = vec![
	"net_version",
	"eth_accounts",
	"eth_blockNumber",
	"eth_call",
	"eth_chainId",
	"eth_estimateGas",
	"eth_gasPrice",
	"eth_getBalance",
	"eth_getBlockByHash",
	"eth_getBlockByNumber",
	"eth_getBlockTransactionCountByHash",
	"eth_getBlockTransactionCountByNumber",
	"eth_getCode",
	"eth_getStorageAt",
	"eth_getTransactionByBlockHashAndIndex",
	"eth_getTransactionByBlockNumberAndIndex",
	"eth_getTransactionByHash",
	"eth_getTransactionCount",
	"eth_getTransactionReceipt",
	"eth_sendRawTransaction",
  ];

  /// Mapping of primitive schema types to their Rust counterparts.
  pub static ref PRIMITIVE_MAPPINGS: HashMap<&'static str, &'static str> = HashMap::from([
	("#/components/schemas/address", "Address"),
	("#/components/schemas/byte", "Byte"),

	("#/components/schemas/bytes", "Bytes"),
	("#/components/schemas/bytes256", "Bytes256"),
	("#/components/schemas/hash32", "H256"),
	("#/components/schemas/bytes32", "H256"),
	("#/components/schemas/bytes8", "String"),
	("#/components/schemas/uint", "U256"),
	("#/components/schemas/uint256", "U256"),
	("#/components/schemas/uint64", "U256"),
  ]);


  /// Mapping of legacy aliases to their new names.
  pub static ref LEGACY_ALIASES: HashMap<&'static str, HashMap<&'static str, &'static str>> = HashMap::from([
	// We accept "data" and "input" for backwards-compatibility reasons.
	// Issue detail: https://github.com/ethereum/go-ethereum/issues/15628
	("#/components/schemas/GenericTransaction", HashMap::from([("input", "data")])),
  ]);
}

/// Read the OpenRPC specs, and inject extra methods and legacy aliases.
pub fn read_specs() -> anyhow::Result<OpenRpc> {
	let content = include_str!("../openrpc.json");
	let mut specs: OpenRpc = serde_json::from_str(content)?;

	// Inject legacy aliases.
	inject_legacy_aliases(&mut specs);

	// Inject extra methods.
	specs.methods.push(RefOr::Inline(Method {
		name: "net_version".to_string(),
		summary: Some("The string value of current network id".to_string()),
		result: Some(RefOr::Reference { reference: "String".to_string() }),
		..Default::default()
	}));

	Ok(specs)
}

// Inject legacy aliases declared by [`LEGACY_ALIASES`].
pub fn inject_legacy_aliases(specs: &mut OpenRpc) {
	for (alias, mapping) in LEGACY_ALIASES.iter() {
		let schema = specs.get_schema_mut(alias).unwrap();
		match &mut schema.contents {
			SchemaContents::Object(o) | SchemaContents::Literal(Literal::Object(o)) => {
				o.legacy_aliases =
					mapping.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
			},
			_ => {
				panic!("Alias should be an object got {:?} instead", schema.contents);
			},
		}
	}
}

/// Type generator for generating RPC methods and types.
#[derive(Default)]
pub struct TypeGenerator {
	/// List of collected types, that are not yet generated.
	collected: BTreeMap<String, ReferenceOrSchema>,
	/// List of already generated types.
	generated: HashSet<String>,
	/// List of filtered method names, we want to generate.
	filtered_method_names: HashSet<String>,
	/// Stripped prefix for the generated method names.
	prefix: String,
}

/// Reference or schema
pub enum ReferenceOrSchema {
	// A reference to a schema such as `#/components/schemas/Foo`.
	Reference(String),
	// A schema definition.
	Schema(Schema),
}

impl ReferenceOrSchema {
	/// Return the schema for the reference or the schema itself.
	fn schema<'a>(&'a self, specs: &'a OpenRpc) -> &'a Schema {
		match self {
			Self::Schema(schema) => schema,
			Self::Reference(reference) => specs.get_schema(reference).unwrap(),
		}
	}
}

impl TypeGenerator {
	/// Create a new type generator.
	pub fn new() -> Self {
		let mut generated =
			HashSet::from_iter(["notFound"].into_iter().map(|name| name.to_pascal_case()));

		generated.extend(PRIMITIVE_MAPPINGS.keys().map(|name| reference_to_name(name)));
		generated.extend(PRIMITIVE_MAPPINGS.values().map(|name| name.to_string()));
		let filtered_method_names =
			SUPPORTED_ETH_METHODS.iter().map(|name| name.to_string()).collect();

		Self {
			collected: Default::default(),
			filtered_method_names,
			generated,
			prefix: "eth".to_string(),
		}
	}

	/// Generate the RPC method, and add the collected types.
	pub fn generate_rpc_methods(&mut self, specs: &OpenRpc) -> String {
		let methods = specs
			.methods
			.iter()
			.map(RefOr::unwrap_inline)
			.filter(|method| self.filtered_method_names.contains(&method.name))
			.collect::<Vec<_>>();

		if methods.len() != self.filtered_method_names.len() {
			let available =
				methods.iter().map(|method| method.name.clone()).collect::<HashSet<_>>();
			let missing = self.filtered_method_names.difference(&available).collect::<Vec<_>>();
			panic!("Missing methods: {missing:?}");
		}

		let mut code = indoc! {r###"
            //! Generated JSON-RPC methods.
            #![allow(missing_docs)]

            use super::*;
            use jsonrpsee::core::RpcResult;
            use jsonrpsee::proc_macros::rpc;

            #[rpc(server, client)]
            pub trait EthRpc {

        "###}
		.to_string();

		for method in methods {
			self.generate_rpc_method(&mut code, method);
			code.push('\n');
		}
		code.push('}');
		code.push('\n');
		code
	}

	pub fn collect_extra_type(&mut self, type_name: &str) {
		self.collect(
			type_name,
			ReferenceOrSchema::Reference(format!("#/components/schemas/{}", type_name)),
		);
	}

	/// Recursively collect the types and generate them.
	///
	/// Note: This should be called after [`TypeGenerator::generate_rpc_methods`] to collect the
	/// types used in the RPC methods.
	pub fn generate_types(&mut self, specs: &OpenRpc) -> String {
		let mut code = indoc! {r###"
            //! Generated JSON-RPC types.
            #![allow(missing_docs)]

			use super::{byte::*, Type0, Type1, Type2};
			use codec::{Decode, Encode};
			use derive_more::{From, TryInto};
			pub use ethereum_types::*;
			use scale_info::TypeInfo;
			use serde::{Deserialize, Serialize};

            #[cfg(not(feature = "std"))]
            use alloc::{string::String, vec::Vec};

        "###}
		.to_string();
		loop {
			let collected = mem::take(&mut self.collected);
			self.generated.extend(collected.keys().cloned());

			if collected.is_empty() {
				break;
			}

			for (name, ref_or_schema) in collected {
				let r#type = self.generate_type(name, ref_or_schema.schema(specs));
				r#type.print(&mut code);
				code.push('\n');
			}
		}

		code
	}

	/// Return the type printer for the given schema.
	fn generate_type(&mut self, name: String, schema: &Schema) -> TypePrinter {
		let doc = doc_str_from_schema(schema);

		let content = match &schema.contents {
			&SchemaContents::Literal(Literal::Object(ref o)) | &SchemaContents::Object(ref o) =>
				TypeContent::Struct(Fields::from(o, self)),
			SchemaContents::AllOf { all_of } =>
				TypeContent::Struct(Fields::from_all_of(all_of, self)),
			&SchemaContents::AnyOf { any_of: ref items } |
			&SchemaContents::OneOf { one_of: ref items } =>
				TypeContent::Enum(Variants::from_one_of(items, self)),
			&SchemaContents::Literal(Literal::Array(ArrayLiteral { items: Some(ref schema) })) => {
				let name = self.type_info(schema).expect("Anonymous array type not supported").name;

				let type_info = TypeInfo { name, required: Required::Yes, array: true };

				TypeContent::TypeAlias(type_info)
			},
			&SchemaContents::Literal(Literal::String(StringLiteral {
				min_length: None,
				max_length: None,
				pattern: None,
				format: None,
				enumeration: Some(ref enumeration),
			})) => TypeContent::UntaggedEnum(enumeration.clone()),
			v => {
				panic!("Unsupported type {name} {v:#?}")
			},
		};

		TypePrinter { name, doc, content }
	}

	fn generate_rpc_method(&mut self, buffer: &mut String, method: &Method) {
		let Method { ref summary, ref name, ref params, ref result, .. } = method;
		writeln!(@doc buffer, summary);

		let result = result
			.as_ref()
			.map(|content| match content {
				RefOr::Inline(descriptor) => self
					.type_info(&descriptor.schema)
					.expect("Result type should be defined")
					.get_type(),
				RefOr::Reference { reference } => reference.clone(),
			})
			.unwrap_or("()".to_string());

		let parameters = params
			.iter()
			.map(RefOr::unwrap_inline)
			.map(|ContentDescriptor { name, required, schema, .. }| {
				let name_arg = name.to_snake_case().replace(' ', "_");
				let name_type = self
					.type_info(schema)
					.expect("Parameter type should be defined")
					.set_required(*required)
					.get_type();
				format!("{name_arg}: {name_type}")
			})
			.collect::<Vec<_>>()
			.join(", ");

		writeln!(buffer, "#[method(name = \"{name}\")]");
		let method_name = name.trim_start_matches(&self.prefix).to_snake_case();
		writeln!(buffer, "async fn {method_name}(&self, {parameters}) -> RpcResult<{result}>;");
	}

	/// Collect the type if it's not yet generated or collected.
	fn collect(&mut self, type_name: &str, ref_or_schema: ReferenceOrSchema) {
		if !self.generated.contains(type_name) && !self.collected.contains_key(type_name) {
			self.collected.insert(type_name.to_string(), ref_or_schema);
		}
	}
}

/// Convert a reference to a type name.
fn reference_to_name(reference: &str) -> String {
	if PRIMITIVE_MAPPINGS.contains_key(reference) {
		return PRIMITIVE_MAPPINGS[reference].to_string();
	}
	reference.split('/').last().unwrap().to_pascal_case()
}

impl TypeNameProvider for TypeGenerator {
	fn type_info(&mut self, schema: &Schema) -> Option<TypeInfo> {
		match &schema.contents {
			SchemaContents::Reference { reference } => {
				let type_name = reference_to_name(reference);
				self.collect(&type_name, ReferenceOrSchema::Reference(reference.to_string()));
				Some(type_name.into())
			},
			SchemaContents::Literal(Literal::Array(ArrayLiteral { items: Some(ref schema) })) => {
				let name = self.type_info(schema).expect("Anonymous array type not supported").name;

				Some(TypeInfo { name, required: Required::Yes, array: true })
			},
			SchemaContents::AllOf { all_of } => Some(
				all_of
					.iter()
					.map(|s| self.type_info(s).expect("Anonymous all_of type not supported").name)
					.collect::<Vec<_>>()
					.join("And")
					.into(),
			),
			SchemaContents::AnyOf { any_of: ref items } |
			SchemaContents::OneOf { one_of: ref items } => {
				let mut required = Required::Yes;
				let items = items
					.iter()
					.filter_map(|s| {
						let info = self.type_info(s).expect("Anonymous any_of type not supported");

						if info.name == "Null" || info.name == "NotFound" {
							required = Required::No { skip_if_null: false };
							None
						} else {
							Some(info.name)
						}
					})
					.collect::<Vec<_>>();

				let name = items.join("Or");
				if items.len() > 1 {
					self.collect(&name, ReferenceOrSchema::Schema(schema.clone()));
				}

				Some(TypeInfo { name, required, array: false })
			},
			SchemaContents::Literal(Literal::Null) => Some("Null".into()),

			// Use Type0, Type1, Type2, ... for String that have a single digit pattern.
			SchemaContents::Literal(Literal::String(StringLiteral {
				min_length: None,
				max_length: None,
				pattern: Some(ref pattern),
				format: None,
				enumeration: None,
			})) if ["^0x0$", "^0x1$", "^0x2$"].contains(&pattern.as_str()) => {
				let type_id = format!("Type{}", &pattern[3..4]);

				Some(type_id.into())
			},

			SchemaContents::Literal(Literal::Boolean) => Some("bool".into()),
			SchemaContents::Object(_) => None,
			v => {
				panic!("No type name for {v:#?}");
			},
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use indoc::indoc;
	use pretty_assertions::assert_eq;

	#[test]
	fn generate_works() {
		let specs = read_specs().unwrap();

		let mut generator = TypeGenerator::new();
		SUPPORTED_ETH_METHODS.iter().for_each(|name| {
			generator.filtered_method_names.insert(name.to_string());
		});

		let buffer = generator.generate_rpc_methods(&specs);
		println!("{}", buffer);
	}

	#[test]
	fn generate_rpc_works() {
		let method = serde_json::from_str::<Method>(
            r###"
            {
            "name": "eth_estimateGas",
            "summary": "Generates and returns an estimate of how much gas is necessary to allow the transaction to complete.",
            "params": [
                {
                "name": "Transaction",
                "required": true,
                "schema": {
                    "$ref": "#/components/schemas/GenericTransaction"
                }
                },
                {
                "name": "Block",
                "required": false,
                "schema": {
                    "$ref": "#/components/schemas/BlockNumberOrTag"
                }
                }
            ],
            "result": {
                "name": "Gas used",
                "schema": {
                "$ref": "#/components/schemas/uint"
                }
            }
            }
            "###,
        )
        .unwrap();

		let mut buffer = String::new();
		let mut generator = TypeGenerator::new();

		generator.generate_rpc_method(&mut buffer, &method);
		assert_eq!(
			buffer,
			indoc! {r#"
            /// Generates and returns an estimate of how much gas is necessary to allow the transaction to complete.
            #[method(name = "eth_estimateGas")]
            async fn estimate_gas(&self, transaction: GenericTransaction, block: Option<BlockNumberOrTag>) -> RpcResult<U256>;
            "#}
		);
	}

	#[test]
	fn generate_type_name_works() {
		let mut generator = TypeGenerator::new();

		let schema: Schema = serde_json::from_str(
			r###"
            {
                "title": "to address",
                "oneOf": [
                    { "title": "Contract Creation (null)", "type": "null" },
                    { "title": "Address", "$ref": "#/components/schemas/address" }
                ]
      }
            "###,
		)
		.unwrap();

		assert_eq!(&generator.type_info(&schema).unwrap().get_type(), "Option<Address>");
	}

	#[test]
	fn generate_all_off_type_works() {
		let specs = read_specs().unwrap();
		let mut generator = TypeGenerator::new();
		let res = generator.generate_type(
			"Transaction4844Signed".to_string(),
			specs.get_schema("#/components/schemas/Transaction4844Signed").unwrap(),
		);
		let mut buffer = String::new();
		res.print(&mut buffer);
		assert_eq!(
			buffer,
			indoc! {r###"
            /// Signed 4844 Transaction
            #[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]
            pub struct Transaction4844Signed {
              #[serde(flatten)]
              pub transaction_4844_unsigned: Transaction4844Unsigned,
              /// r
              pub r: U256,
              /// s
              pub s: U256,
              /// yParity
              /// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
              #[serde(rename = "yParity", skip_serializing_if = "Option::is_none")]
              pub y_parity: Option<U256>,
            }
            "###}
		);
	}

	#[test]
	fn generate_one_of_type_works() {
		let specs = read_specs().unwrap();
		let mut generator = TypeGenerator::new();
		let res = generator.generate_type(
			"TransactionUnsigned".to_string(),
			specs.get_schema("#/components/schemas/TransactionUnsigned").unwrap(),
		);
		let mut buffer = String::new();
		res.print(&mut buffer);
		assert_eq!(
			buffer,
			indoc! {r###"
              #[derive(Debug, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, From, TryInto, Eq, PartialEq)]
              #[serde(untagged)]
              pub enum TransactionUnsigned {
                Transaction4844Unsigned(Transaction4844Unsigned),
                Transaction1559Unsigned(Transaction1559Unsigned),
                Transaction2930Unsigned(Transaction2930Unsigned),
                TransactionLegacyUnsigned(TransactionLegacyUnsigned),
              }
              impl Default for TransactionUnsigned {
                fn default() -> Self {
                  TransactionUnsigned::Transaction4844Unsigned(Default::default())
                }
              }
            "###}
		);
	}

	#[test]
	fn generate_array_type_works() {
		let specs = read_specs().unwrap();
		let mut generator = TypeGenerator::new();
		let res = generator.generate_type(
			"AccessList".to_string(),
			specs.get_schema("#/components/schemas/AccessList").unwrap(),
		);
		let mut buffer = String::new();
		res.print(&mut buffer);
		assert_eq!(
			buffer,
			indoc! {r###"
            /// Access list
            pub type AccessList = Vec<AccessListEntry>;
            "###}
		);
	}

	#[test]
	fn generate_one_of_with_null_variant_works() {
		let specs = read_specs().unwrap();
		let mut generator = TypeGenerator::new();
		let res = generator.generate_type(
			"FilterTopics".to_string(),
			specs.get_schema("#/components/schemas/FilterTopics").unwrap(),
		);
		let mut buffer = String::new();
		res.print(&mut buffer);
		assert_eq!(
			buffer,
			indoc! {r###"
            /// Filter Topics
            pub type FilterTopics = Vec<FilterTopic>;
            "###}
		);
	}

	#[test]
	fn generate_object_type_works() {
		let specs = read_specs().unwrap();
		let mut generator = TypeGenerator::new();
		let res = generator.generate_type(
			"Transaction".to_string(),
			specs.get_schema("#/components/schemas/GenericTransaction").unwrap(),
		);

		let mut buffer = String::new();
		res.print(&mut buffer);
		assert_eq!(
			buffer,
			indoc! {r###"
            /// Transaction object generic to all types
            #[derive(Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq)]
            pub struct Transaction {
              /// accessList
              /// EIP-2930 access list
              #[serde(rename = "accessList", skip_serializing_if = "Option::is_none")]
              pub access_list: Option<AccessList>,
              /// blobVersionedHashes
              /// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
              #[serde(rename = "blobVersionedHashes", skip_serializing_if = "Option::is_none")]
              pub blob_versioned_hashes: Option<Vec<H256>>,
              /// blobs
              /// Raw blob data.
              #[serde(skip_serializing_if = "Option::is_none")]
              pub blobs: Option<Vec<Bytes>>,
              /// chainId
              /// Chain ID that this transaction is valid on.
              #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
              pub chain_id: Option<U256>,
              /// from address
              #[serde(skip_serializing_if = "Option::is_none")]
              pub from: Option<Address>,
              /// gas limit
              #[serde(skip_serializing_if = "Option::is_none")]
              pub gas: Option<U256>,
              /// gas price
              /// The gas price willing to be paid by the sender in wei
              #[serde(rename = "gasPrice", skip_serializing_if = "Option::is_none")]
              pub gas_price: Option<U256>,
              /// input data
              #[serde(alias = "data", skip_serializing_if = "Option::is_none")]
              pub input: Option<Bytes>,
              /// max fee per blob gas
              /// The maximum total fee per gas the sender is willing to pay for blob gas in wei
              #[serde(rename = "maxFeePerBlobGas", skip_serializing_if = "Option::is_none")]
              pub max_fee_per_blob_gas: Option<U256>,
              /// max fee per gas
              /// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei
              #[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
              pub max_fee_per_gas: Option<U256>,
              /// max priority fee per gas
              /// Maximum fee per gas the sender is willing to pay to miners in wei
              #[serde(rename = "maxPriorityFeePerGas", skip_serializing_if = "Option::is_none")]
              pub max_priority_fee_per_gas: Option<U256>,
              /// nonce
              #[serde(skip_serializing_if = "Option::is_none")]
              pub nonce: Option<U256>,
              /// to address
              pub to: Option<Address>,
              /// type
              #[serde(skip_serializing_if = "Option::is_none")]
              pub r#type: Option<Byte>,
              /// value
              #[serde(skip_serializing_if = "Option::is_none")]
              pub value: Option<U256>,
            }
            "###}
		);
	}
}
