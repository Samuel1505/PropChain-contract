#![cfg_attr(not(feature = "std"), no_std)]

use ink::prelude::*;
use ink::storage::Mapping;
use ink::primitives::AccountId;
use propchain_traits::*;

#[ink::contract]
mod propchain_contracts {
    use super::*;

    /// Error types for contract
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        PropertyNotFound,
        Unauthorized,
        InvalidMetadata,
        EscrowNotFound,
        EscrowAlreadyReleased,
        InsufficientFunds,
    }

    /// Property Registry contract
    #[ink(storage)]
    pub struct PropertyRegistry {
        /// Mapping from property ID to property information
        properties: Mapping<u64, PropertyInfo>,
        /// Mapping from owner to their properties
        owner_properties: Mapping<AccountId, Vec<u64>>,
        /// Property counter
        property_count: u64,
        /// Mapping from escrow ID to escrow information
        escrows: Mapping<u64, EscrowInfo>,
        /// Escrow counter
        escrow_count: u64,
    }

    #[ink(event)]
    pub struct PropertyRegistered {
        #[ink(topic)]
        property_id: u64,
        owner: AccountId,
    }

    #[ink(event)]
    pub struct PropertyTransferred {
        #[ink(topic)]
        property_id: u64,
        from: AccountId,
        to: AccountId,
    }

    /// Escrow information
    #[derive(Debug, Clone, PartialEq, scale::Encode, scale::Decode, ink::storage::traits::StorageLayout)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct EscrowInfo {
        pub id: u64,
        pub property_id: u64,
        pub buyer: AccountId,
        pub seller: AccountId,
        pub amount: u128,
        pub released: bool,
    }

    #[ink(event)]
    pub struct EscrowCreated {
        #[ink(topic)]
        escrow_id: u64,
        property_id: u64,
        buyer: AccountId,
        seller: AccountId,
        amount: u128,
    }

    #[ink(event)]
    pub struct EscrowReleased {
        #[ink(topic)]
        escrow_id: u64,
    }

    #[ink(event)]
    pub struct EscrowRefunded {
        #[ink(topic)]
        escrow_id: u64,
    }

    impl PropertyRegistry {
        /// Creates a new PropertyRegistry contract
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                properties: Mapping::default(),
                owner_properties: Mapping::default(),
                property_count: 0,
                escrows: Mapping::default(),
                escrow_count: 0,
            }
        }

        /// Registers a new property
        #[ink(message)]
        pub fn register_property(&mut self, metadata: PropertyMetadata) -> Result<u64, Error> {
            let caller = self.env().caller();
            self.property_count += 1;
            let property_id = self.property_count;

            let property_info = PropertyInfo {
                id: property_id,
                owner: caller,
                metadata,
                registered_at: self.env().block_timestamp(),
            };

            self.properties.insert(&property_id, &property_info);

            let mut owner_props = self.owner_properties.get(&caller).unwrap_or_default();
            owner_props.push(property_id);
            self.owner_properties.insert(&caller, &owner_props);

            self.env().emit_event(PropertyRegistered {
                property_id,
                owner: caller,
            });

            Ok(property_id)
        }

        /// Transfers property ownership
        #[ink(message)]
        pub fn transfer_property(&mut self, property_id: u64, to: AccountId) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            if property.owner != caller {
                return Err(Error::Unauthorized);
            }

            // Remove from current owner's properties
            let mut current_owner_props = self.owner_properties.get(&caller).unwrap_or_default();
            current_owner_props.retain(|&id| id != property_id);
            self.owner_properties.insert(&caller, &current_owner_props);

            // Add to new owner's properties
            let mut new_owner_props = self.owner_properties.get(&to).unwrap_or_default();
            new_owner_props.push(property_id);
            self.owner_properties.insert(&to, &new_owner_props);

            // Update property owner
            property.owner = to;
            self.properties.insert(&property_id, &property);

            self.env().emit_event(PropertyTransferred {
                property_id,
                from: caller,
                to,
            });

            Ok(())
        }

        /// Gets property information
        #[ink(message)]
        pub fn get_property(&self, property_id: u64) -> Option<PropertyInfo> {
            self.properties.get(&property_id)
        }

        /// Gets properties owned by an account
        #[ink(message)]
        pub fn get_owner_properties(&self, owner: AccountId) -> Vec<u64> {
            self.owner_properties.get(&owner).unwrap_or_default()
        }

        /// Gets total property count
        #[ink(message)]
        pub fn property_count(&self) -> u64 {
            self.property_count
        }

        /// Creates a new escrow for property transfer
        #[ink(message)]
        pub fn create_escrow(&mut self, property_id: u64, amount: u128) -> Result<u64, Error> {
            let caller = self.env().caller();
            let property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            // Only property owner can create escrow
            if property.owner != caller {
                return Err(Error::Unauthorized);
            }

            self.escrow_count += 1;
            let escrow_id = self.escrow_count;

            let escrow_info = EscrowInfo {
                id: escrow_id,
                property_id,
                buyer: caller, // In this simple version, caller is buyer
                seller: property.owner,
                amount,
                released: false,
            };

            self.escrows.insert(&escrow_id, &escrow_info);

            self.env().emit_event(EscrowCreated {
                escrow_id,
                property_id,
                buyer: caller,
                seller: property.owner,
                amount,
            });

            Ok(escrow_id)
        }

        /// Releases escrow funds and transfers property
        #[ink(message)]
        pub fn release_escrow(&mut self, escrow_id: u64) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut escrow = self.escrows.get(&escrow_id).ok_or(Error::EscrowNotFound)?;

            if escrow.released {
                return Err(Error::EscrowAlreadyReleased);
            }

            // Only buyer can release
            if escrow.buyer != caller {
                return Err(Error::Unauthorized);
            }

            // Transfer property
            self.transfer_property(escrow.property_id, escrow.buyer)?;

            escrow.released = true;
            self.escrows.insert(&escrow_id, &escrow);

            self.env().emit_event(EscrowReleased {
                escrow_id,
            });

            Ok(())
        }

        /// Refunds escrow funds
        #[ink(message)]
        pub fn refund_escrow(&mut self, escrow_id: u64) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut escrow = self.escrows.get(&escrow_id).ok_or(Error::EscrowNotFound)?;

            if escrow.released {
                return Err(Error::EscrowAlreadyReleased);
            }

            // Only seller can refund
            if escrow.seller != caller {
                return Err(Error::Unauthorized);
            }

            escrow.released = true;
            self.escrows.insert(&escrow_id, &escrow);

            self.env().emit_event(EscrowRefunded {
                escrow_id,
            });

            Ok(())
        }

        /// Gets escrow information
        #[ink(message)]
        pub fn get_escrow(&self, escrow_id: u64) -> Option<EscrowInfo> {
            self.escrows.get(&escrow_id)
        }
    }

    impl Default for PropertyRegistry {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Escrow for PropertyRegistry {
        type Error = Error;

        fn create_escrow(&mut self, property_id: u64, amount: u128) -> Result<u64, Self::Error> {
            self.create_escrow(property_id, amount)
        }

        fn release_escrow(&mut self, escrow_id: u64) -> Result<(), Self::Error> {
            self.release_escrow(escrow_id)
        }

        fn refund_escrow(&mut self, escrow_id: u64) -> Result<(), Self::Error> {
            self.refund_escrow(escrow_id)
        }
    }
}

#[cfg(test)]
mod tests;
