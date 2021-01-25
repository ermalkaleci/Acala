//! # Evm Accounts Module
//!
//! ## Overview
//!
//! Evm Accounts module provide a two way mapping between Substrate accounts and
//! EVM accounts so user only have deal with one account / private key.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage, ensure,
	traits::{Currency, HandleLifetime, IsType, OnKilledAccount, ReservableCurrency},
	transactional,
	weights::Weight,
	StorageMap,
};
use frame_system::ensure_signed;
use orml_traits::account::MergeAccount;
use primitives::{
	evm::{AddressMapping, EvmAddress},
	AccountIndex,
};
use sp_core::{crypto::AccountId32, ecdsa};
use sp_io::{
	crypto::secp256k1_ecdsa_recover,
	hashing::{blake2_256, keccak_256},
};
use sp_runtime::{
	traits::{LookupError, StaticLookup},
	MultiAddress,
};
use sp_std::{marker::PhantomData, vec::Vec};

mod default_weight;
mod mock;
mod tests;

pub trait WeightInfo {
	fn claim_account() -> Weight;
}

pub type EcdsaSignature = ecdsa::Signature;

pub trait Config: frame_system::Config {
	type Event: From<Event<Self>> + Into<<Self as frame_system::Config>::Event>;

	/// The Currency for managing Evm account assets.
	type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

	/// Mapping from address to account id.
	type AddressMapping: AddressMapping<Self::AccountId>;

	/// Merge free balance from source to dest.
	type MergeAccount: MergeAccount<Self::AccountId>;

	/// Handler to kill account in system.
	type KillAccount: HandleLifetime<Self::AccountId>;

	/// Weight information for the extrinsics in this module.
	type WeightInfo: WeightInfo;
}

decl_event!(
	pub enum Event<T> where
		<T as frame_system::Config>::AccountId,
		EvmAddress = EvmAddress,
	{
		/// Mapping between Substrate accounts and EVM accounts
		/// claim account. \[account_id, evm_address\]
		ClaimAccount(AccountId, EvmAddress),
	}
);

decl_error! {
	/// Error for evm accounts module.
	pub enum Error for Module<T: Config> {
		/// AccountId has mapped
		AccountIdHasMapped,
		/// Eth address has mapped
		EthAddressHasMapped,
		/// Bad signature
		BadSignature,
		/// Invalid signature
		InvalidSignature,
		/// Account ref count is not zero
		NonZeroRefCount,
		/// Account still has active reserved
		StillHasActiveReserved,
	}
}

decl_storage! {
	trait Store for Module<T: Config> as EvmAccounts {
		pub Accounts get(fn accounts): map hasher(twox_64_concat) EvmAddress => Option<T::AccountId>;
		pub EvmAddresses get(fn evm_addresses): map hasher(twox_64_concat) T::AccountId => Option<EvmAddress>;
	}
}

decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		type Error = Error<T>;
		fn deposit_event() = default;

		/// Claim account mapping between Substrate accounts and EVM accounts.
		/// Ensure eth_address has not been mapped.
		#[weight = T::WeightInfo::claim_account()]
		#[transactional]
		pub fn claim_account(origin, eth_address: EvmAddress, eth_signature: EcdsaSignature) {
			let who = ensure_signed(origin)?;

			// ensure account_id and eth_address has not been mapped
			ensure!(!EvmAddresses::<T>::contains_key(&who), Error::<T>::AccountIdHasMapped);
			ensure!(!Accounts::<T>::contains_key(eth_address), Error::<T>::EthAddressHasMapped);

			// recover evm address from signature
			let address = Self::eth_recover(&eth_signature, &who.using_encoded(to_ascii_hex), &[][..]).ok_or(Error::<T>::BadSignature)?;
			ensure!(eth_address == address, Error::<T>::InvalidSignature);

			// check if the evm padded address already exists
			let account_id = T::AddressMapping::get_account_id(&eth_address);
			if frame_system::Account::<T>::contains_key(&account_id) {
				// merge balance from `evm padded address` to `origin`
				T::MergeAccount::merge_account(&account_id, &who)?;
				// finally kill the account
				T::KillAccount::killed(&account_id);
			}

			Accounts::<T>::insert(eth_address, &who);
			EvmAddresses::<T>::insert(&who, eth_address);

			Self::deposit_event(RawEvent::ClaimAccount(who, eth_address));
		}
	}
}

impl<T: Config> Module<T> {
	// Constructs the message that Ethereum RPC's `personal_sign` and `eth_sign`
	// would sign.
	pub fn ethereum_signable_message(what: &[u8], extra: &[u8]) -> Vec<u8> {
		let prefix = b"acala evm:";
		let mut l = prefix.len() + what.len() + extra.len();
		let mut rev = Vec::new();
		while l > 0 {
			rev.push(b'0' + (l % 10) as u8);
			l /= 10;
		}
		let mut v = b"\x19Ethereum Signed Message:\n".to_vec();
		v.extend(rev.into_iter().rev());
		v.extend_from_slice(&prefix[..]);
		v.extend_from_slice(what);
		v.extend_from_slice(extra);
		v
	}

	// Attempts to recover the Ethereum address from a message signature signed by
	// using the Ethereum RPC's `personal_sign` and `eth_sign`.
	pub fn eth_recover(s: &EcdsaSignature, what: &[u8], extra: &[u8]) -> Option<EvmAddress> {
		let msg = keccak_256(&Self::ethereum_signable_message(what, extra));
		let mut res = EvmAddress::default();
		res.0
			.copy_from_slice(&keccak_256(&secp256k1_ecdsa_recover(&s.0, &msg).ok()?[..])[12..]);
		Some(res)
	}

	pub fn eth_public(secret: &secp256k1::SecretKey) -> secp256k1::PublicKey {
		secp256k1::PublicKey::from_secret_key(secret)
	}

	pub fn eth_address(secret: &secp256k1::SecretKey) -> EvmAddress {
		EvmAddress::from_slice(&keccak_256(&Self::eth_public(secret).serialize()[1..65])[12..])
	}

	pub fn eth_sign(secret: &secp256k1::SecretKey, what: &[u8], extra: &[u8]) -> EcdsaSignature {
		let msg = keccak_256(&Self::ethereum_signable_message(&to_ascii_hex(what)[..], extra));
		let (sig, recovery_id) = secp256k1::sign(&secp256k1::Message::parse(&msg), secret);
		let mut r = [0u8; 65];
		r[0..64].copy_from_slice(&sig.serialize()[..]);
		r[64] = recovery_id.serialize();
		EcdsaSignature::from_slice(&r)
	}
}

fn account_to_default_evm_address(account_id: &impl Encode) -> EvmAddress {
	let payload = (b"evm:", account_id);
	EvmAddress::from_slice(&payload.using_encoded(blake2_256)[0..20])
}

pub struct EvmAddressMapping<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> AddressMapping<T::AccountId> for EvmAddressMapping<T>
where
	T::AccountId: IsType<AccountId32>,
{
	fn get_account_id(address: &EvmAddress) -> T::AccountId {
		if let Some(acc) = Accounts::<T>::get(address) {
			acc
		} else {
			let mut data: [u8; 32] = [0u8; 32];
			data[0..4].copy_from_slice(b"evm:");
			data[4..24].copy_from_slice(&address[..]);
			AccountId32::from(data).into()
		}
	}

	fn get_evm_address(account_id: &T::AccountId) -> Option<EvmAddress> {
		EvmAddresses::<T>::get(account_id).or_else(|| {
			let data: &[u8] = account_id.into_ref().as_ref();
			if data.starts_with(b"evm:") {
				Some(EvmAddress::from_slice(&data[4..24]))
			} else {
				None
			}
		})
	}

	fn get_or_create_evm_address(account_id: &T::AccountId) -> EvmAddress {
		Self::get_evm_address(account_id).unwrap_or_else(|| {
			let addr = account_to_default_evm_address(account_id);

			// create reverse mapping
			Accounts::<T>::insert(&addr, account_id);

			addr
		})
	}

	fn get_default_evm_address(account_id: &T::AccountId) -> EvmAddress {
		account_to_default_evm_address(account_id)
	}

	fn is_linked(account_id: &T::AccountId, evm: &EvmAddress) -> bool {
		Self::get_evm_address(account_id).as_ref() == Some(evm)
			|| &account_to_default_evm_address(account_id.into_ref()) == evm
	}
}

pub struct CallKillAccount<T>(PhantomData<T>);
impl<T: Config> OnKilledAccount<T::AccountId> for CallKillAccount<T> {
	fn on_killed_account(who: &T::AccountId) {
		// remove the reserve mapping that could be created by
		// `get_or_create_evm_address`
		Accounts::<T>::remove(account_to_default_evm_address(who.into_ref()));

		// remove mapping created by `claim_account`
		if let Some(evm_addr) = Module::<T>::evm_addresses(who) {
			Accounts::<T>::remove(evm_addr);
			EvmAddresses::<T>::remove(who);
		}
	}
}

impl<T: Config> StaticLookup for Module<T> {
	type Source = MultiAddress<T::AccountId, AccountIndex>;
	type Target = T::AccountId;

	fn lookup(a: Self::Source) -> Result<Self::Target, LookupError> {
		match a {
			MultiAddress::Address20(i) => Ok(T::AddressMapping::get_account_id(&EvmAddress::from_slice(&i))),
			_ => Err(LookupError),
		}
	}

	fn unlookup(a: Self::Target) -> Self::Source {
		MultiAddress::Id(a)
	}
}

/// Converts the given binary data into ASCII-encoded hex. It will be twice the
/// length.
pub fn to_ascii_hex(data: &[u8]) -> Vec<u8> {
	let mut r = Vec::with_capacity(data.len() * 2);
	let mut push_nibble = |n| r.push(if n < 10 { b'0' + n } else { b'a' - 10 + n });
	for &b in data.iter() {
		push_nibble(b / 16);
		push_nibble(b % 16);
	}
	r
}
