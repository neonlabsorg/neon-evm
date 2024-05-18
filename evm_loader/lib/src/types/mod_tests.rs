use super::*;
use std::str::FromStr;

use crate::tracing::ChainBalanceOverride;
use crate::types::tracer_ch_common::RevisionMap;

#[test]
fn test_build_ranges_empty() {
    let results = Vec::new();
    let exp = Vec::new();
    let res = RevisionMap::build_ranges(results);
    assert_eq!(res, exp);
}

#[test]
fn test_build_ranges_single_element() {
    let results = vec![(1u64, String::from("Rev1"))];
    let exp = vec![(1u64, 2u64, String::from("Rev1"))];
    let res = RevisionMap::build_ranges(results);
    assert_eq!(res, exp);
}

#[test]
fn test_build_ranges_multiple_elements_different_revision() {
    let results = vec![
        (222222222u64, String::from("Rev1")),
        (333333333u64, String::from("Rev2")),
        (444444444u64, String::from("Rev3")),
    ];

    let exp = vec![
        (222222222u64, 333333333u64, String::from("Rev1")),
        (333333334u64, 444444444u64, String::from("Rev2")),
        (444444445u64, 444444445u64, String::from("Rev3")),
    ];
    let res = RevisionMap::build_ranges(results);

    assert_eq!(res, exp);
}

#[test]
fn test_rangemap() {
    let ranges = vec![
        (123456780, 123456788, String::from("Rev1")),
        (123456789, 123456793, String::from("Rev2")),
        (123456794, 123456799, String::from("Rev3")),
    ];
    let map = RevisionMap::new(ranges);

    assert_eq!(map.get(123456779), None); // Below the bottom bound of the first range

    assert_eq!(map.get(123456780), Some(String::from("Rev1"))); // The bottom bound of the first range
    assert_eq!(map.get(123456785), Some(String::from("Rev1"))); // Within the first range
    assert_eq!(map.get(123456788), Some(String::from("Rev1"))); // The top bound of the first range

    assert_eq!(map.get(123456793), Some(String::from("Rev2"))); // The bottom bound of the second range
    assert_eq!(map.get(123456790), Some(String::from("Rev2"))); // Within the second range
    assert_eq!(map.get(123456793), Some(String::from("Rev2"))); // The top bound of the second range

    assert_eq!(map.get(123456799), Some(String::from("Rev3"))); // The bottom bound of the third range
    assert_eq!(map.get(123456795), Some(String::from("Rev3"))); // Within the third range
    assert_eq!(map.get(123456799), Some(String::from("Rev3"))); // The top bound of the third range

    assert_eq!(map.get(123456800), None); // Beyond the top end of the last range
}

#[test]
fn test_deserialize() {
    let txt = r#"
    {
        "step_limit": 500000,
        "accounts": [],
        "chains": [
            {
                "id": 111,
                "name": "neon",
                "token": "HPsV9Deocecw3GeZv1FkAPNCBRfuVyfw9MMwjwRe1xaU"
            },
            {
                "id": 112,
                "name": "sol",
                "token": "So11111111111111111111111111111111111111112"
            },
            {
                "id": 113,
                "name": "usdt",
                "token": "2duuuuhNJHUYqcnZ7LKfeufeeTBgSJdftf2zM3cZV6ym"
            },
            {
                "id": 114,
                "name": "eth",
                "token": "EwJYd3UAFAgzodVeHprB2gMQ68r4ZEbbvpoVzCZ1dGq5"
            }
        ],
        "tx": {
            "from": "0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99",
            "to": "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883",
            "value": "0x0",
            "data": "3ff21f8e",
            "chain_id": 111
        },
        "solana_overrides": {
            "EwJYd3UAFAgzodVeHprB2gMQ68r4ZEbbvpoVzCZ1dGq5": null,
            "2duuuuhNJHUYqcnZ7LKfeufeeTBgSJdftf2zM3cZV6ym": {
                "lamports": 1000000000000,
                "owner": "So11111111111111111111111111111111111111112",
                "executable": false,
                "rent_epoch": 0,
                "data": "0102030405"
            }
        }
    }
    "#;

    let request: super::EmulateRequest = serde_json::from_str(txt).unwrap();
    println!("{:?}", request);
    assert!(request.chains.is_some());
    assert_eq!(request.chains.unwrap().len(), 4);
    assert_eq!(
        request.tx.from,
        Address::from_str("0x3fd219e7cf0e701fcf5a6903b40d47ca4e597d99").unwrap()
    );
    assert_eq!(
        request.tx.to,
        Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883").ok()
    );
    assert!(request.solana_overrides.is_some());
    let binding = request.solana_overrides.unwrap();
    let key = Pubkey::from_str("EwJYd3UAFAgzodVeHprB2gMQ68r4ZEbbvpoVzCZ1dGq5").unwrap();
    let ewjyd3uafagzodvehprb2gmq68r4zebbvpovzcz1dgq5 = binding.get(&key);
    assert!(ewjyd3uafagzodvehprb2gmq68r4zebbvpovzcz1dgq5.is_some());
    assert!(ewjyd3uafagzodvehprb2gmq68r4zebbvpovzcz1dgq5
        .unwrap()
        .is_none());
    let duuuuhnjhuyqcnz7lkfeufeetbgsjdftf2zm3czv6ym =
        binding.get(&Pubkey::from_str("2duuuuhNJHUYqcnZ7LKfeufeeTBgSJdftf2zM3cZV6ym").unwrap());
    assert!(duuuuhnjhuyqcnz7lkfeufeetbgsjdftf2zm3czv6ym.is_some());
    let data = duuuuhnjhuyqcnz7lkfeufeetbgsjdftf2zm3czv6ym.unwrap();
    assert_eq!(data.as_ref().unwrap().lamports, 1000000000000);
}

fn check<F>(json: &str, key: &str, f: F) -> bool
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

    let request = serde_json::from_str::<EmulateRequest>(&payload).unwrap();
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
            "balanceOverrides": {
                "0x0673ac30e9c5dd7955ae9fb7e46b3cddca435883": {}
            }
        }
    }
    "#,
        )
        .unwrap();
        assert!(request.trace_config.is_some());
        let trace_call_config = request.trace_config.unwrap();
        assert!(trace_call_config.balance_overrides.is_some());
        let balance_overrides = trace_call_config.balance_overrides.unwrap();
        assert_eq!(balance_overrides.len(), 1);
        assert!(check(
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
    }
}
