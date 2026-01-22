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
    }

    /// Property Registry contract
    #[ink(storage)]
    pub struct PropertyRegistry {
        /// Mapping from property ID to property information
        properties: Mapping<u64, PropertyInfo>,
        /// Mapping from owner to their properties
        owner_properties: Mapping<AccountId, Vec<u64>>,
        /// Mapping from property ID to approved account
        approvals: Mapping<u64, AccountId>,
        /// Property counter
        property_count: u64,
    }

    #[ink(event)]
    pub struct PropertyRegistered {
        #[ink(topic)]
        property_id: u64,
        #[ink(topic)]
        owner: AccountId,
        version: u8,
    }

    #[ink(event)]
    pub struct PropertyTransferred {
        #[ink(topic)]
        property_id: u64,
        #[ink(topic)]
        from: AccountId,
        #[ink(topic)]
        to: AccountId,
    }

    #[ink(event)]
    pub struct PropertyMetadataUpdated {
        #[ink(topic)]
        property_id: u64,
        metadata: PropertyMetadata,
    }

    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        property_id: u64,
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        approved: AccountId,
    }

    impl PropertyRegistry {
        /// Creates a new PropertyRegistry contract
        #[ink(constructor)]
        pub fn new() -> Self {
            Self {
                properties: Mapping::default(),
                owner_properties: Mapping::default(),
                approvals: Mapping::default(),
                property_count: 0,
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
                version: 1,
            });

            Ok(property_id)
        }

        /// Transfers property ownership
        #[ink(message)]
        pub fn transfer_property(&mut self, property_id: u64, to: AccountId) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            let approved = self.approvals.get(&property_id);
            if property.owner != caller && Some(caller) != approved {
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

            // Clear approval
            self.approvals.remove(&property_id);

            self.env().emit_event(PropertyTransferred {
                property_id,
                from: property.owner, // Use recorded owner as from, in case caller is approved
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

        /// Updates property metadata
        #[ink(message)]
        pub fn update_metadata(&mut self, property_id: u64, metadata: PropertyMetadata) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            if property.owner != caller {
                return Err(Error::Unauthorized);
            }

            // check if metadata is valid (basic check)
            if metadata.location.is_empty() {
                return Err(Error::InvalidMetadata);
            }

            property.metadata = metadata.clone();
            self.properties.insert(&property_id, &property);

            self.env().emit_event(PropertyMetadataUpdated {
                property_id,
                metadata,
            });

            Ok(())
        }

        /// Approves an account to transfer a specific property
        #[ink(message)]
        pub fn approve(&mut self, property_id: u64, to: Option<AccountId>) -> Result<(), Error> {
            let caller = self.env().caller();
            let property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            if property.owner != caller {
                return Err(Error::Unauthorized);
            }

            if let Some(account) = to {
                self.approvals.insert(&property_id, &account);
                self.env().emit_event(Approval {
                    property_id,
                    owner: caller,
                    approved: account,
                });
            } else {
                self.approvals.remove(&property_id);
                 // We could emit an approval with 0x0 or special handling, 
                 // but for now let's just emit if setting a new approval. 
                 // Or we should emit approval to 0 account if clearing?
                 // Let's assume Option<AccountId> maps to clearing.
                 // For the event, we need an AccountId.
                 // Let's rely on the fact that 'to' is Option.
                 // If we strictly follow ERC721 style, we should emit 0 address.
                 // But ink! AccountId is 32 bytes.
                 // Let's just not emit if clearing for simplicity or emit to zero account if preferred.
                 // Re-reading requirements: "Structured event emission...".
                 // Let's emit with a zero account for None.
                let zero_account = AccountId::from([0u8; 32]);
                self.env().emit_event(Approval {
                    property_id,
                    owner: caller,
                    approved: zero_account,
                });
            }

            Ok(())
        }

        /// Gets the approved account for a property
        #[ink(message)]
        pub fn get_approved(&self, property_id: u64) -> Option<AccountId> {
            self.approvals.get(&property_id)
        }
    }

    impl Default for PropertyRegistry {
        fn default() -> Self {
            Self::new()
        }
    }
}



#[cfg(test)]
mod tests;
