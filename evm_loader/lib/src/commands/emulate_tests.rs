use super::*;
use ethnum::U256;
use evm_loader::types::Address;
use std::collections::HashMap;

#[tokio::test]
async fn test_parse_balance_overrides_from_payload() {
    let expected_nonce = 17;
    let expected_balance = U256::MAX;

    let address1 = Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555").unwrap();
    let address2 = Address::from_str("0x0673ac30e9c5dd7955ae9fb7e46b3cddca444444").unwrap();

    let chain_balance_override1 = ChainBalanceOverride {
        address: Some(address1),
        chain_id: Some(111),
        nonce: Some(expected_nonce),
        balance: Some(expected_balance),
    };

    let chain_balance_override2 = ChainBalanceOverride {
        address: Some(address2),
        chain_id: Some(222),
        nonce: Some(expected_nonce),
        balance: Some(expected_balance),
    };

    let expected_chain_balance = ChainBalanceOverrides::from([
        chain_balance_override1.clone(),
        chain_balance_override2.clone(),
    ]);

    {
        // Address and chainId passed inside data structure like:
        // balanceOverrides: {
        //    "address1@chainId": {
        //       "address": "address1",    <- address is taken from the structure
        //       "chain_id": "chain_id1",  <- chain id is taken from the structure
        //       "nonce": "nonce1",
        //       "balance": "balance1",
        //
        //    }
        // }
        let mut tracer_config_data = HashMap::<String, ChainBalanceOverride>::new();
        tracer_config_data.insert(String::from("1"), chain_balance_override1.clone());
        tracer_config_data.insert(String::from("2"), chain_balance_override2.clone());

        assert_eq!(
            parse_balance_overrides(Some(tracer_config_data)),
            Some(expected_chain_balance.clone())
        );
    }
    {
        // Address and chainId are taken from the key parameter
        // balanceOverrides: {
        //    "address1@chainId": {  <- address and chain id are parsed from the key
        //       "nonce": "nonce1",
        //       "balance": "balance1",
        //    }
        // }
        let mut tracer_config_data = HashMap::<String, ChainBalanceOverride>::new();
        tracer_config_data.insert(
            String::from("0x0673ac30e9c5dd7955ae9fb7e46b3cddca455555@111"),
            ChainBalanceOverride {
                address: None,
                chain_id: None,
                nonce: Some(expected_nonce),
                balance: Some(expected_balance),
            },
        );
        tracer_config_data.insert(
            String::from("0x0673ac30e9c5dd7955ae9fb7e46b3cddca444444@222"),
            ChainBalanceOverride {
                address: None,
                chain_id: None,
                nonce: Some(expected_nonce),
                balance: Some(expected_balance),
            },
        );

        assert_eq!(
            parse_balance_overrides(Some(tracer_config_data)),
            Some(expected_chain_balance.clone())
        );
    }
}
