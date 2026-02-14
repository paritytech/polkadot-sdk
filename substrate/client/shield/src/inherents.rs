use sp_inherents::{Error, InherentData, InherentIdentifier};
use sp_runtime::BoundedVec;
use stp_shield::{INHERENT_IDENTIFIER, InherentType, ShieldKeystorePtr};

pub struct InherentDataProvider {
    keystore: ShieldKeystorePtr,
}

impl InherentDataProvider {
    pub fn new(keystore: ShieldKeystorePtr) -> Self {
        Self { keystore }
    }
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
    async fn provide_inherent_data(&self, inherent_data: &mut InherentData) -> Result<(), Error> {
        let public_key = self.keystore.next_public_key().ok();
        let bounded = public_key.map(|pk| BoundedVec::truncate_from(pk));
        inherent_data.put_data::<InherentType>(INHERENT_IDENTIFIER, &bounded)
    }

    async fn try_handle_error(
        &self,
        _: &InherentIdentifier,
        _: &[u8],
    ) -> Option<Result<(), Error>> {
        None
    }
}
