use crate::{
    matchengine::slfe::{
        Slfe,
        iceberg_manager::{IcebergOrderConfig, IcebergOrderEvent},
    },
    order::Order,
    types::UnifiedResult,
};

pub struct IcebergOrderProcessor;

impl IcebergOrderProcessor {
    pub async fn handle(slfe: &Slfe, iceberg_order: Order) -> UnifiedResult<String> {
        slfe.iceberg.event_tx.send(IcebergOrderEvent::Create {
            order: iceberg_order,
            config: IcebergOrderConfig::default(),
        });
        Ok("".to_string())
    }
}
