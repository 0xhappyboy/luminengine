use std::sync::Arc;

use crate::{
    matchengine::slfe::{
        Slfe,
        manager::iceberg::{IcebergOrderConfig, IcebergOrderEvent},
    },
    order::Order,
    types::UnifiedResult,
};

pub struct IcebergOrderProcessor;

impl IcebergOrderProcessor {
    pub   fn handle(slfe: Arc<Slfe>, iceberg_order: Order) -> UnifiedResult<String> {
        slfe.iceberg_manager
            .event_tx
            .send(IcebergOrderEvent::Create {
                order: iceberg_order,
                config: IcebergOrderConfig::default(),
            });
        Ok("".to_string())
    }
}
