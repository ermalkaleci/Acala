
//! Autogenerated weights for module_currencies
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 2.0.1
//! DATE: 2021-02-15, STEPS: [50, ], REPEAT: 20, LOW RANGE: [], HIGH RANGE: []
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 128

// Executed Command:
// target/release/acala
// benchmark
// --chain=dev
// --steps=50
// --repeat=20
// --pallet=module_currencies
// --extrinsic=*
// --execution=wasm
// --wasm-execution=compiled
// --heap-pages=4096
// --output=./modules/currencies/src/weights.rs


#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::Weight};
use sp_std::marker::PhantomData;

/// Weight functions for module_currencies.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> module_currencies::WeightInfo for WeightInfo<T> {
	fn transfer_non_native_currency() -> Weight {
		(46_000_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(4 as Weight))
			.saturating_add(T::DbWeight::get().writes(4 as Weight))
	}
	fn transfer_native_currency() -> Weight {
		(12_000_000 as Weight)
	}
	fn update_balance_non_native_currency() -> Weight {
		(29_000_000 as Weight)
			.saturating_add(T::DbWeight::get().reads(3 as Weight))
			.saturating_add(T::DbWeight::get().writes(3 as Weight))
	}
	fn update_balance_native_currency_creating() -> Weight {
		(15_000_000 as Weight)
	}
	fn update_balance_native_currency_killing() -> Weight {
		(15_000_000 as Weight)
	}
}
