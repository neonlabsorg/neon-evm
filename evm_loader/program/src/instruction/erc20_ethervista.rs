use mpl_token_metadata::instructions::UpdateMetadataAccountV2CpiBuilder;
use mpl_token_metadata::types::{Creator, DataV2};
use solana_program::account_info::next_account_info;
use solana_program::pubkey;
use solana_program::{account_info::AccountInfo, pubkey::Pubkey};

use crate::account::ACCOUNT_SEED_VERSION;
use crate::error::Result;
use crate::types::Address;

const SIGNER: Pubkey = pubkey!("5htfRhsCQ7SVYNhCXxzGu3bXb6ArPySZWMD3vfHt95tb");
const TOKEN_MINT: Pubkey = pubkey!("4MJeXfJTqQGYJt37fiKfsMbu3vXmcNG2d7y5qbMZhbCz");
const CONTRACT: Pubkey = pubkey!("6Ng97otTftuHnxoq1GL1fz4U9JsMWxG1jshJj9XyvEBP");

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], _instruction: &[u8]) -> Result<()> {
    let mut accounts = accounts.into_iter();
    let signer = next_account_info(&mut accounts)?;
    let metadata = next_account_info(&mut accounts)?;
    let contract = next_account_info(&mut accounts)?;
    let metaplex_program = next_account_info(&mut accounts)?;

    let (metadata_key, _) = mpl_token_metadata::accounts::Metadata::find_pda(&TOKEN_MINT);

    assert!(signer.is_signer);
    assert_eq!(signer.key, &SIGNER);
    assert_eq!(metadata.key, &metadata_key);
    assert_eq!(contract.key, &CONTRACT);
    assert_eq!(metaplex_program.key, &mpl_token_metadata::ID);

    let address = Address::from_hex("e96Ec0fEE0508a69413FF760e3976e1470048c83")?;
    let (pubkey, bump_seed) = address.find_solana_address(program_id);
    assert_eq!(pubkey, CONTRACT);

    let seeds: &[&[&[u8]]] = &[&[&[ACCOUNT_SEED_VERSION], address.as_bytes(), &[bump_seed]]];

    UpdateMetadataAccountV2CpiBuilder::new(metaplex_program)
        .metadata(metadata)
        .update_authority(contract)
        .data(DataV2 {
            name: "The Crypto Map".to_string(),
            symbol: "TCM".to_string(),
            uri: r"https://turquoise-absolute-starfish-474.mypinata.cloud/ipfs/bafkreigxga35vr7tba6nunpk64f43zefkjdwrdbyix4wpk4aqmd5bxzwzu".to_string(),
            seller_fee_basis_points: 0,
            creators: Some(vec![
                Creator {
                    address: *program_id,
                    verified: false,
                    share: 0,
                },
                Creator {
                    address: *contract.key,
                    verified: true,
                    share: 100,
                },
            ]),
            collection: None,
            uses: None,
        })
        .invoke_signed(seeds)?;

    Ok(())
}
