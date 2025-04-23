mod deactivated_features_tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use solana_account_decoder::UiDataSliceConfig as SliceConfig;
    use solana_client::client_error::Result as ClientResult;
    use solana_sdk::{
        account::{Account, AccountSharedData},
        feature::Feature,
        feature_set,
        pubkey::Pubkey,
    };

    use crate::{
        rpc::Rpc,
        types::deactivated_features::{
            get_deactivated_features_at_slot, set_deactivated_features_rpc,
        },
    };

    #[derive(Clone)]
    struct RpcMockFeatures {
        pub features: HashMap<Pubkey, Option<u64>>,
    }

    impl Default for RpcMockFeatures {
        fn default() -> Self {
            let accounts: Vec<_> = feature_set::FEATURE_NAMES.keys().copied().collect();

            let mut features = HashMap::<Pubkey, Option<u64>>::new();

            for (slot, account) in accounts.iter().enumerate() {
                let slot = if slot == 0 { None } else { Some(slot as u64) };
                features.insert(*account, slot);
            }

            Self { features }
        }
    }

    #[async_trait(?Send)]
    impl Rpc for RpcMockFeatures {
        async fn get_account_slice(
            &self,
            _key: &Pubkey,
            _slice: Option<SliceConfig>,
        ) -> ClientResult<Option<Account>> {
            todo!()
        }

        async fn get_multiple_accounts(
            &self,
            pubkeys: &[Pubkey],
        ) -> ClientResult<Vec<Option<Account>>> {
            let mut result: Vec<Option<Account>> = vec![];

            for pubkey in pubkeys {
                let feature = self.features.get(pubkey);

                match feature {
                    Some(slot) => {
                        let mut data =
                            AccountSharedData::new(1_000_000_000, 100, &solana_sdk::feature::ID);
                        if solana_sdk::feature::to_account(
                            &Feature {
                                activated_at: *slot,
                            },
                            &mut data,
                        )
                        .is_some()
                        {
                            result.push(Some(data.into()));
                        } else {
                            result.push(None);
                        }
                    }
                    None => result.push(None),
                }
            }

            Ok(result)
        }

        async fn get_deactivated_solana_features(&self) -> ClientResult<Vec<Pubkey>> {
            todo!()
        }
    }

    async fn get_deactivated_features_and_wait(slot: Option<u64>) -> Vec<Pubkey> {
        get_deactivated_features_at_slot(slot)
            .await
            .expect("get deactivated features failed at slot unexpectedly")
    }

    // by default? mock rpc contains vector of features: RpcMockFeatures::features
    // features[0] is not activated
    // features[1] is activated at slot 1
    // features[i] is activated at slot i
    // let we have N features overall, then...
    // only 1 feature will be deactivated on slot N (features[0])
    // only 2 features will be deactivated on slot N-1 (features[0], features[n-1] (last one) which should be activated at slot N)
    // ...
    // all features will be deactivated on slot 0
    #[tokio::test]
    async fn test_deactivated_features_cache() {
        let rpc = RpcMockFeatures::default();
        let features_cnt = rpc.features.len() as u64;
        set_deactivated_features_rpc(rpc.clone()).await;

        for i in 0..features_cnt {
            let features = get_deactivated_features_and_wait(Some(i)).await;

            assert_eq!(features_cnt - i, features.len() as u64);
        }
        {
            // current slot
            let features = get_deactivated_features_and_wait(None).await;
            assert_eq!(features.len(), 1);
        }
    }
}
