//! Copied from https://github.com/o1-labs/proof-systems/blob/932fa7e6429f8160586d13efcaf72fccc3fc53ac/signer/tests/transaction.rs
//! since it's defined in test and not accessable for now.

use mina_hasher::{Hashable, ROInput};
use mina_p2p_messages::{
    bigint::BigInt,
    number::Number,
    string::ByteString,
    v1::{
        ConsensusGlobalSlotStableV1VersionedV1PolyArg0V1,
        ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8,
        ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1,
        ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1Poly,
        ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1PolyPolyV1,
        CurrencyAmountMakeStrStableV1VersionedV1, CurrencyFeeStableV1VersionedV1,
        MinaBasePaymentPayloadStableV1VersionedV1, MinaBasePaymentPayloadStableV1VersionedV1PolyV1,
        MinaBaseSignatureStableV1VersionedV1, MinaBaseSignatureStableV1VersionedV1PolyV1,
        MinaBaseSignedCommandMemoStableV1VersionedV1,
        MinaBaseSignedCommandPayloadBodyBinableArgStableV1VersionedV1,
        MinaBaseSignedCommandPayloadBodyStableV1VersionedV1,
        MinaBaseSignedCommandPayloadCommonBinableArgStableV1VersionedV1,
        MinaBaseSignedCommandPayloadCommonBinableArgStableV1VersionedV1PolyV1,
        MinaBaseSignedCommandPayloadCommonStableV1VersionedV1,
        MinaBaseSignedCommandPayloadStableV1VersionedV1,
        MinaBaseSignedCommandPayloadStableV1VersionedV1PolyV1,
        MinaBaseSignedCommandStableV1VersionedV1, MinaBaseSignedCommandStableV1VersionedV1PolyV1,
        MinaBaseTokenIdStableV1VersionedV1, MinaBaseUserCommandStableV1VersionedV1,
        MinaBaseUserCommandStableV1VersionedV1PolyV1, MinaNumbersNatMake32StableV1VersionedV1,
        MinaNumbersNatMake64StableV1VersionedV1,
        NetworkPoolTransactionPoolDiffVersionedStableV1VersionedV1,
        NonZeroCurvePointUncompressedStableV1Versioned,
        NonZeroCurvePointUncompressedStableV1VersionedV1,
        UnsignedExtendedUInt32StableV1VersionedV1, UnsignedExtendedUInt64StableV1VersionedV1,
    },
    GossipNetMessageV1,
};
use mina_signer::{CompressedPubKey, Keypair, NetworkId, PubKey, Signature};

const MEMO_BYTES: usize = 34;
const TAG_BITS: usize = 3;
const PAYMENT_TX_TAG: [bool; TAG_BITS] = [false, false, false];
const DELEGATION_TX_TAG: [bool; TAG_BITS] = [false, false, true];

#[derive(Clone)]
pub struct Transaction {
    // Common
    pub fee: u64,
    pub fee_token: u64,
    pub fee_payer_pk: CompressedPubKey,
    pub nonce: u32,
    pub valid_until: u32,
    pub memo: [u8; MEMO_BYTES],
    // Body
    pub tag: [bool; TAG_BITS],
    pub source_pk: CompressedPubKey,
    pub receiver_pk: CompressedPubKey,
    pub token_id: u64,
    pub amount: u64,
    pub token_locked: bool,
}

impl Hashable for Transaction {
    type D = NetworkId;

    fn to_roinput(&self) -> ROInput {
        let mut roi = ROInput::new()
            .append_field(self.fee_payer_pk.x)
            .append_field(self.source_pk.x)
            .append_field(self.receiver_pk.x)
            .append_u64(self.fee)
            .append_u64(self.fee_token)
            .append_bool(self.fee_payer_pk.is_odd)
            .append_u32(self.nonce)
            .append_u32(self.valid_until)
            .append_bytes(&self.memo);

        for tag_bit in self.tag {
            roi = roi.append_bool(tag_bit);
        }

        roi.append_bool(self.source_pk.is_odd)
            .append_bool(self.receiver_pk.is_odd)
            .append_u64(self.token_id)
            .append_u64(self.amount)
            .append_bool(self.token_locked)
    }

    fn domain_string(network_id: NetworkId) -> Option<String> {
        // Domain strings must have length <= 20
        match network_id {
            NetworkId::MAINNET => "MinaSignatureMainnet",
            NetworkId::TESTNET => "CodaSignature",
        }
        .to_string()
        .into()
    }
}

impl Transaction {
    pub fn new_payment(from: PubKey, to: PubKey, amount: u64, fee: u64, nonce: u32) -> Self {
        Transaction {
            fee: fee,
            fee_token: 1,
            fee_payer_pk: from.into_compressed(),
            nonce: nonce,
            // TODO(zura): was u32::MAX?
            valid_until: i32::MAX as u32,
            memo: std::array::from_fn(|i| (i == 0) as u8),
            tag: PAYMENT_TX_TAG,
            source_pk: from.into_compressed(),
            receiver_pk: to.into_compressed(),
            token_id: 1,
            amount: amount,
            token_locked: false,
        }
    }

    // pub fn new_delegation(from: PubKey, to: PubKey, fee: u64, nonce: u32) -> Self {
    //     Transaction {
    //         fee: fee,
    //         fee_token: 1,
    //         fee_payer_pk: from.into_compressed(),
    //         nonce: nonce,
    //         valid_until: u32::MAX,
    //         memo: std::array::from_fn(|i| (i == 0) as u8),
    //         tag: DELEGATION_TX_TAG,
    //         source_pk: from.into_compressed(),
    //         receiver_pk: to.into_compressed(),
    //         token_id: 1,
    //         amount: 0,
    //         token_locked: false,
    //     }
    // }

    pub fn set_valid_until(mut self, global_slot: u32) -> Self {
        self.valid_until = global_slot;

        self
    }

    pub fn set_memo(mut self, memo: [u8; MEMO_BYTES - 2]) -> Self {
        self.memo[0] = 0x01;
        self.memo[1] = (MEMO_BYTES - 2) as u8;
        self.memo[2..].copy_from_slice(&memo[..]);

        self
    }

    pub fn set_memo_str(mut self, memo: &str) -> Self {
        self.memo[0] = 0x01;
        self.memo[1] = std::cmp::min(memo.len(), MEMO_BYTES - 2) as u8;
        let memo = format!("{:\0<32}", memo); // Pad user-supplied memo with zeros
        self.memo[2..]
            .copy_from_slice(&memo.as_bytes()[..std::cmp::min(memo.len(), MEMO_BYTES - 2)]);
        // Anything beyond MEMO_BYTES is truncated

        self
    }

    fn pub_key_to_p2p_type(
        key: CompressedPubKey,
    ) -> ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8 {
        let v =
            ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1PolyPolyV1 {
                x: BigInt::from(key.x),
                is_odd: key.is_odd,
            };
        let v =
            ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1Poly(v.into());
        ConsensusProofOfStakeDataConsensusStateValueStableV1VersionedV1PolyArg8V1(v).into()
    }

    pub fn to_gossipsub_v1_msg(self, sig: Signature) -> GossipNetMessageV1 {
        let from = Self::pub_key_to_p2p_type(self.source_pk.clone());
        let to = Self::pub_key_to_p2p_type(self.receiver_pk.clone());

        let v = UnsignedExtendedUInt64StableV1VersionedV1(Number(self.fee_token as i64));
        let v = MinaNumbersNatMake64StableV1VersionedV1(v.into());
        let fee_token_id = MinaBaseTokenIdStableV1VersionedV1(v.into());

        let v = UnsignedExtendedUInt64StableV1VersionedV1(Number(self.fee as i64));
        let fee = CurrencyFeeStableV1VersionedV1(v.into());

        let v = UnsignedExtendedUInt32StableV1VersionedV1(Number(self.nonce as i32));
        let nonce = MinaNumbersNatMake32StableV1VersionedV1(v.into());

        let v = UnsignedExtendedUInt32StableV1VersionedV1(Number(self.valid_until as i32));
        let valid_until_slot = ConsensusGlobalSlotStableV1VersionedV1PolyArg0V1(v.into());

        // TODO(zura): add from_bytes method to ByteString.
        let v = ByteString::from(self.memo.to_vec());
        let memo = MinaBaseSignedCommandMemoStableV1VersionedV1(v);

        let v = MinaBaseSignedCommandPayloadCommonBinableArgStableV1VersionedV1PolyV1 {
            fee: fee.into(),
            fee_token: fee_token_id.into(),
            fee_payer_pk: from.clone(),
            nonce: nonce.into(),
            valid_until: valid_until_slot.into(),
            memo: memo.into(),
        };
        let v = MinaBaseSignedCommandPayloadCommonBinableArgStableV1VersionedV1(v.into());
        let common = MinaBaseSignedCommandPayloadCommonStableV1VersionedV1(v.into());

        let v = UnsignedExtendedUInt64StableV1VersionedV1(Number(self.token_id as i64));
        let v = MinaNumbersNatMake64StableV1VersionedV1(v.into());
        let token_id = MinaBaseTokenIdStableV1VersionedV1(v.into());

        let v = UnsignedExtendedUInt64StableV1VersionedV1(Number(self.amount as i64));
        let amount = CurrencyAmountMakeStrStableV1VersionedV1(v.into());

        let v = MinaBasePaymentPayloadStableV1VersionedV1PolyV1 {
            source_pk: from.clone(),
            receiver_pk: to.clone(),
            token_id: token_id.into(),
            amount: amount.into(),
        };
        let v = MinaBasePaymentPayloadStableV1VersionedV1(v.into());
        let v = MinaBaseSignedCommandPayloadBodyBinableArgStableV1VersionedV1::Payment(v.into());
        let body = MinaBaseSignedCommandPayloadBodyStableV1VersionedV1(v.into());

        let v = MinaBaseSignedCommandPayloadStableV1VersionedV1PolyV1 {
            common: common.into(),
            body: body.into(),
        };
        let payload = MinaBaseSignedCommandPayloadStableV1VersionedV1(v.into());

        let v = MinaBaseSignatureStableV1VersionedV1PolyV1(sig.rx.into(), sig.s.into());
        let signature = MinaBaseSignatureStableV1VersionedV1(v.into());

        let v = MinaBaseSignedCommandStableV1VersionedV1PolyV1 {
            payload: payload.into(),
            signer: NonZeroCurvePointUncompressedStableV1VersionedV1(from.clone()).into(),
            signature: signature.into(),
        };
        let v = MinaBaseSignedCommandStableV1VersionedV1(v.into());
        let v = MinaBaseUserCommandStableV1VersionedV1PolyV1::SignedCommand(v.into());
        let v = MinaBaseUserCommandStableV1VersionedV1(v.into());
        let v = NetworkPoolTransactionPoolDiffVersionedStableV1VersionedV1(vec![v.into()]);
        GossipNetMessageV1::TransactionPoolDiff(v.into())
    }
}
