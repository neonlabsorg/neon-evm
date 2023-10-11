use crate::{
    commands::collect_treasury::{self, CollectTreasuryReturn},
    Config, Context, NeonResult,
};

pub async fn execute(context: &Context<'_>, config: &Config) -> NeonResult<CollectTreasuryReturn> {
    collect_treasury::execute(config, context).await
}
