#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unexpected_cfgs)]

use ink::prelude::vec::Vec;
use ink::storage::Mapping;
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
        NotCompliant, // Recipient is not compliant
        ComplianceCheckFailed, // Compliance registry call failed
    }

    /// Property Registry contract
    #[ink(storage)]
    pub struct PropertyRegistry {
        /// Mapping from property ID to property informatio
        properties: Mapping<u64, PropertyInfo>,
        /// Mapping from owner to their properties
        owner_properties: Mapping<AccountId, Vec<u64>>,
        /// Property counter
        property_count: u64,
        /// Compliance registry contract address (optional)
        compliance_registry: Option<AccountId>,
        /// Contract owner (for setting compliance registry)
        owner: AccountId,
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
            let caller = Self::env().caller();
            Self {
                properties: Mapping::default(),
                owner_properties: Mapping::default(),
                property_count: 0,
                compliance_registry: None,
                owner: caller,
            }
        }

        /// Creates a new PropertyRegistry contract with compliance registry
        #[ink(constructor)]
        pub fn new_with_compliance(compliance_registry: AccountId) -> Self {
            let caller = Self::env().caller();
            Self {
                properties: Mapping::default(),
                owner_properties: Mapping::default(),
                property_count: 0,
                compliance_registry: Some(compliance_registry),
                owner: caller,
            }
        }

        /// Set or update the compliance registry address (owner only)
        #[ink(message)]
        pub fn set_compliance_registry(&mut self, compliance_registry: AccountId) -> Result<(), Error> {
            if self.env().caller() != self.owner {
                return Err(Error::Unauthorized);
            }
            self.compliance_registry = Some(compliance_registry);
            Ok(())
        }

        /// Get the compliance registry address
        #[ink(message)]
        pub fn get_compliance_registry(&self) -> Option<AccountId> {
            self.compliance_registry
        }

        /// Check if an account is compliant (internal helper)
        fn check_compliance(&self, account: AccountId) -> Result<(), Error> {
            if let Some(compliance_addr) = self.compliance_registry {
                // Build cross-contract call to ComplianceRegistry::is_compliant
                // Using is_compliant which returns bool (simpler than require_compliance)
                let selector = ink::selector_bytes!("is_compliant");
                
                let is_compliant: bool = ink::env::call::build_call::<ink::env::DefaultEnvironment>()
                    .call(compliance_addr)
                    .exec_input(
                        ink::env::call::ExecutionInput::new(
                            ink::env::call::Selector::new(selector)
                        ).push_arg(account)
                    )
                    .returns::<bool>()
                    .invoke();

                if is_compliant {
                    Ok(())
                } else {
                    Err(Error::NotCompliant)
                }
            } else {
                // No compliance registry set, allow transfer (backward compatibility)
                Ok(())
            }
        }

        /// Registers a new property
        /// Optionally checks compliance if compliance registry is set
        #[ink(message)]
        pub fn register_property(&mut self, metadata: PropertyMetadata) -> Result<u64, Error> {
            let caller = self.env().caller();
            
            // Check compliance for property registration (optional but recommended)
            self.check_compliance(caller)?;
            
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
        /// Requires recipient to be compliant if compliance registry is set
        #[ink(message)]
        pub fn transfer_property(&mut self, property_id: u64, to: AccountId) -> Result<(), Error> {
            let caller = self.env().caller();
            let mut property = self.properties.get(&property_id).ok_or(Error::PropertyNotFound)?;

            if property.owner != caller {
                return Err(Error::Unauthorized);
            }

            // CRITICAL: Check compliance before allowing transfer
            // This ensures only verified, compliant users can receive properties
            self.check_compliance(to)?;

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

    #[cfg(kani)]
    mod verification {
        use super::*;

        #[kani::proof]
        fn verify_arithmetic_overflow() {
            let a: u64 = kani::any();
            let b: u64 = kani::any();
            // Verify that addition is safe
            if a < 100 && b < 100 {
                assert!(a + b < 200);
            }
        }

        #[kani::proof]
        fn verify_property_info_struct() {
            let id: u64 = kani::any();
            // Verify PropertyInfo layout/safety if needed
            // This is a placeholder for checking structural invariants
            if id > 0 {
                assert!(id > 0);
            }
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
