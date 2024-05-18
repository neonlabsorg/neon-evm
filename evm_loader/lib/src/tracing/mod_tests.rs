use crate::types::EmulateRequest;

use super::*;
use ethnum::U256;
use std::str::FromStr;

fn check_balance_parsing<F>(json: &str, key: &str, f: F) -> bool
where
    F: FnOnce(&ChainBalanceOverride) -> bool,
{
    let payload = r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        },
        "trace_config": {
            "trace_config": {
                "enable_memory": true,
                "disable_storage": true,
                "disable_stack": true,
                "enable_return_data": true,
                "limit": 1
            },
            {balance}
        }
    }
    "#
    .replace("{balance}", json);
    let request: EmulateRequest = serde_json::from_str(&payload).expect("Parsing input data");
    assert!(request.trace_config.is_some());
    let trace_call_config = request.trace_config.unwrap();
    assert!(trace_call_config.balance_overrides.is_some());
    let binding = trace_call_config
        .balance_overrides
        .expect("Failed to extract balance chain overrides");
    let data = binding.get(key);
    assert!(data.as_ref().is_some());
    f(data.unwrap())
}

#[test]
fn test_deserialization_of_balance_overrides() {
    assert!(serde_json::from_str::<EmulateRequest>(
        r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        }
    }
    "#
    )
    .unwrap()
    .trace_config
    .is_none());

    assert!(serde_json::from_str::<EmulateRequest>(
        r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        },
        "trace_config": null
    }
    "#
    )
    .unwrap()
    .trace_config
    .is_none());
    {
        let request = serde_json::from_str::<EmulateRequest>(
            r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        },
        "trace_config": {
            "trace_config": {
                "enable_memory": true,
                "disable_storage": true,
                "disable_stack": true,
                "enable_return_data": true,
                "limit": 1
            }
        }
    }
    "#,
        )
        .unwrap();
        assert!(request.trace_config.is_some());
        assert!(request.trace_config.unwrap().balance_overrides.is_none());
    }

    {
        let request = serde_json::from_str::<EmulateRequest>(
            r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        },
        "trace_config": {
            "trace_config": {
                "enable_memory": true,
                "disable_storage": true,
                "disable_stack": true,
                "enable_return_data": true,
                "limit": 1
            },
            "balanceOverrides": {}
        }
    }
    "#,
        )
        .unwrap();
        assert!(request.trace_config.is_some());
        let trace_call_config = request.trace_config.unwrap();
        assert!(trace_call_config.balance_overrides.is_some());
        let balance_overrides = trace_call_config.balance_overrides.unwrap();
        assert_eq!(balance_overrides.len(), 0);
    }

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883": {}
        }"#,
        "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
        |data| {
            assert!(data.address.is_none());
            assert!(data.chain_id.is_none());
            assert!(data.nonce.is_none());
            assert!(data.balance.is_none());
            true
        }
    ));

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "1": {
                "address": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555"
            }
        }"#,
        "1",
        |data| {
            assert!(data.address.is_some());
            assert_eq!(
                data.address.unwrap(),
                Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555").unwrap()
            );
            assert!(data.chain_id.is_none());
            assert!(data.nonce.is_none());
            assert!(data.balance.is_none());
            true
        }
    ));

    // Address/ChainId inside ChainBalanceOverride payload has bigger priority over
    // Address/ChainId from map key
    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222": {
                "address": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555",
                "chainId": 111
            }
        }"#,
        "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222",
        |data| {
            assert!(data.address.is_some());
            assert_eq!(
                data.address.unwrap(),
                Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555").unwrap()
            );
            assert!(data.chain_id.is_some());
            assert_eq!(data.chain_id.unwrap(), 111);
            assert!(data.nonce.is_none());
            assert!(data.balance.is_none());
            true
        }
    ));

    {
        assert!(check_balance_parsing(
            r#""balanceOverrides": {
            "1": {
                "balance": "{balance}"
            }
        }"#
            .replace(
                "{balance}",
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            )
            .as_ref(),
            "1",
            |data| {
                assert!(data.address.is_none());
                assert!(data.chain_id.is_none());
                assert!(data.nonce.is_none());
                assert!(data.balance.is_some());
                assert_eq!(data.balance.unwrap(), U256::MAX);

                true
            }
        ));
    }
    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222": {
                "nonce": 11
            }
        }"#,
        "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222",
        |data| {
            assert!(data.address.is_none());
            assert!(data.chain_id.is_none());
            assert!(data.nonce.is_some());
            assert_eq!(data.nonce.unwrap(), 11);
            assert!(data.balance.is_none());
            true
        }
    ));

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "1": {
                "balance": "0x22"
            }
        }"#,
        "1",
        |data| {
            assert!(data.address.is_none());
            assert!(data.chain_id.is_none());
            assert!(data.nonce.is_none());
            assert!(data.balance.is_some());
            assert_eq!(data.balance.unwrap(), 0x22);

            true
        }
    ));

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222": {
                "address": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555",
                "chainId": 111,
                "balance": "0x22",
                "nonce": 11
            }
        }"#,
        "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222",
        |data| {
            assert!(data.address.is_some());
            assert_eq!(
                data.address.unwrap(),
                Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555").unwrap()
            );

            assert!(data.chain_id.is_some());
            assert_eq!(data.chain_id.unwrap(), 111);

            assert!(data.nonce.is_some());
            assert_eq!(data.nonce.unwrap(), 11);

            assert!(data.balance.is_some());
            assert_eq!(data.balance.unwrap(), 0x22);

            true
        }
    ));
}
