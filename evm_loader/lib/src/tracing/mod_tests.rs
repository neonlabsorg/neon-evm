use super::*;
use crate::types::EmulateRequest;
use log::info;

fn check_balance_parsing<F>(json: &str, f: F) -> Result<bool, serde_json::Error>
where
    F: FnOnce(&ChainBalanceOverrides) -> bool,
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
    info!("json {json}");
    let request: EmulateRequest = serde_json::from_str(&payload)?;
    assert!(request.trace_config.is_some());
    let trace_call_config = request.trace_config.unwrap();
    assert!(trace_call_config.balance_overrides.is_some());
    let binding = trace_call_config
        .balance_overrides
        .expect("Failed to extract balance chain overrides");
    Ok(f(&binding))
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

    let expected_key = ChainBalanceOverrideKey {
        address: Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555").unwrap(),
        chain_id: 222_u64,
    };

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883": {}
        }"#,
        |data: &ChainBalanceOverrides| {
            assert_eq!(data.len(), 1);
            true
        }
    )
    .is_err());

    // Address/ChainId inside ChainBalanceOverride payload has bigger priority over
    // Address/ChainId from map key
    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883@222": {
            }
        }"#,
        |data: &ChainBalanceOverrides| {
            let value = data.get(&expected_key);
            assert!(value.is_none());
            true
        }
    )
    .is_ok());

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555@222": {
                "nonce": 11
            }
        }"#,
        |data: &ChainBalanceOverrides| {
            let value = data.get(&expected_key);
            assert!(value.is_some());
            assert!(value.unwrap().nonce.is_some());
            assert_eq!(value.unwrap().nonce.unwrap(), 11);
            assert!(value.unwrap().balance.is_none());
            true
        }
    )
    .is_ok());

    assert!(check_balance_parsing(
        r#""balanceOverrides": {
            "0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555@222": {
                "nonce": 11,
                "balance": "0x22"
            }
        }"#,
        |data: &ChainBalanceOverrides| {
            let value = data.get(&expected_key);
            assert!(value.is_some());
            assert!(value.unwrap().nonce.is_some());
            assert_eq!(value.unwrap().nonce.unwrap(), 11);
            assert_eq!(value.unwrap().balance.unwrap(), 0x22);
            true
        }
    )
    .is_ok());
}
