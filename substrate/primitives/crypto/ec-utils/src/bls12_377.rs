use ark_bls12_377_ext::{Config as ConfigHost, CurveHooks};

pub type Config = ConfigHost<crate::Host>;

pub mod g1 {
	use ark_bls12_377_ext::g1::{
		Config as ConfigHost, G1Affine as G1AffineHost, G1Projective as G1ProjectiveHost,
		G1SWAffine as G1SWAffineHost, G1TEAffine as G1TEAffineHost,
		G1TEProjective as G1TEProjectiveHost,
	};

	pub type Config = ConfigHost<crate::Host>;

	pub type G1Affine = G1AffineHost<crate::Host>;
	pub type G1Projective = G1ProjectiveHost<crate::Host>;

	pub type G1SWAffine = G1SWAffineHost<crate::Host>;
	pub type G1TEAffine = G1TEAffineHost<crate::Host>;
	pub type G1TEProjective = G1TEProjectiveHost<crate::Host>;

	pub use ark_bls12_377_ext::g1::{
		G1_GENERATOR_X, G1_GENERATOR_Y, TE_GENERATOR_X, TE_GENERATOR_Y,
	};
}

pub mod g2 {
	use ark_bls12_377_ext::g2::{
		Config as ConfigHost, G2Affine as G2AffineHost, G2Projective as G2ProjectiveHost,
	};

	pub type G2Affine = G2AffineHost<crate::Host>;
	pub type G2Projective = G2ProjectiveHost<crate::Host>;

	pub type Config = ConfigHost<crate::Host>;

	pub use ark_bls12_377_ext::g2::{
		G2_GENERATOR_X, G2_GENERATOR_X_C0, G2_GENERATOR_X_C1, G2_GENERATOR_Y, G2_GENERATOR_Y_C0,
		G2_GENERATOR_Y_C1,
	};
}
