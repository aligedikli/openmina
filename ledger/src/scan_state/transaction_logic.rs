use ark_ff::{One, Zero};
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use mina_hasher::Fp;
use mina_signer::CompressedPubKey;

use crate::{
    scan_state::transaction_logic::transaction_applied::{CommandApplied, Varying},
    staged_ledger::sparse_ledger::{LedgerIntf, SparseLedger},
    Account, AccountId, BaseLedger, PermissionTo, ReceiptChainHash, Timing, TokenId,
    VerificationKey,
};

use self::{
    local_state::{CallStack, LocalStateEnv, StackFrame},
    protocol_state::{GlobalState, ProtocolStateView},
    signed_command::{SignedCommand, SignedCommandPayload},
    transaction_applied::{
        signed_command_applied::{self, SignedCommandApplied},
        TransactionApplied, ZkappCommandApplied,
    },
    transaction_union_payload::TransactionUnionPayload,
    zkapp_command::{
        AccountPreconditions, AccountUpdate, WithHash, ZkAppCommand, ZkAppPreconditions,
    },
};

use super::{
    currency::{Amount, Balance, BlockTime, Fee, Index, Length, Magnitude, Nonce, Signed, Slot},
    fee_excess::FeeExcess,
    scan_state::{transaction_snark::OneOrTwo, ConstraintConstants},
    zkapp_logic::{apply, Handler, IsStart, StartData},
};

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/transaction_status.ml#L9
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionFailure {
    Predicate,
    SourceNotPresent,
    ReceiverNotPresent,
    AmountInsufficientToCreateAccount,
    CannotPayCreationFeeInToken,
    SourceInsufficientBalance,
    SourceMinimumBalanceViolation,
    ReceiverAlreadyExists,
    TokenOwnerNotCaller,
    Overflow,
    GlobalExcessOverflow,
    LocalExcessOverflow,
    LocalSupplyIncreaseOverflow,
    GlobalSupplyIncreaseOverflow,
    SignedCommandOnZkappAccount,
    ZkappAccountNotPresent,
    UpdateNotPermittedBalance,
    UpdateNotPermittedTimingExistingAccount,
    UpdateNotPermittedDelegate,
    UpdateNotPermittedAppState,
    UpdateNotPermittedVerificationKey,
    UpdateNotPermittedSequenceState,
    UpdateNotPermittedZkappUri,
    UpdateNotPermittedTokenSymbol,
    UpdateNotPermittedPermissions,
    UpdateNotPermittedNonce,
    UpdateNotPermittedVotingFor,
    ZkappCommandReplayCheckFailed,
    FeePayerNonceMustIncrease,
    FeePayerMustBeSigned,
    AccountBalancePreconditionUnsatisfied,
    AccountNoncePreconditionUnsatisfied,
    AccountReceiptChainHashPreconditionUnsatisfied,
    AccountDelegatePreconditionUnsatisfied,
    AccountSequenceStatePreconditionUnsatisfied,
    AccountAppStatePreconditionUnsatisfied(i64),
    AccountProvedStatePreconditionUnsatisfied,
    AccountIsNewPreconditionUnsatisfied,
    ProtocolStatePreconditionUnsatisfied,
    IncorrectNonce,
    InvalidFeeExcess,
    Cancelled,
}

impl ToString for TransactionFailure {
    fn to_string(&self) -> String {
        match self {
            Self::Predicate => "Predicate".to_string(),
            Self::SourceNotPresent => "Source_not_present".to_string(),
            Self::ReceiverNotPresent => "Receiver_not_present".to_string(),
            Self::AmountInsufficientToCreateAccount => {
                "Amount_insufficient_to_create_account".to_string()
            }
            Self::CannotPayCreationFeeInToken => "Cannot_pay_creation_fee_in_token".to_string(),
            Self::SourceInsufficientBalance => "Source_insufficient_balance".to_string(),
            Self::SourceMinimumBalanceViolation => "Source_minimum_balance_violation".to_string(),
            Self::ReceiverAlreadyExists => "Receiver_already_exists".to_string(),
            Self::TokenOwnerNotCaller => "Token_owner_not_caller".to_string(),
            Self::Overflow => "Overflow".to_string(),
            Self::GlobalExcessOverflow => "Global_excess_overflow".to_string(),
            Self::LocalExcessOverflow => "Local_excess_overflow".to_string(),
            Self::LocalSupplyIncreaseOverflow => "Local_supply_increase_overflow".to_string(),
            Self::GlobalSupplyIncreaseOverflow => "Global_supply_increase_overflow".to_string(),
            Self::SignedCommandOnZkappAccount => "Signed_command_on_zkapp_account".to_string(),
            Self::ZkappAccountNotPresent => "Zkapp_account_not_present".to_string(),
            Self::UpdateNotPermittedBalance => "Update_not_permitted_balance".to_string(),
            Self::UpdateNotPermittedTimingExistingAccount => {
                "Update_not_permitted_timing_existing_account".to_string()
            }
            Self::UpdateNotPermittedDelegate => "update_not_permitted_delegate".to_string(),
            Self::UpdateNotPermittedAppState => "Update_not_permitted_app_state".to_string(),
            Self::UpdateNotPermittedVerificationKey => {
                "Update_not_permitted_verification_key".to_string()
            }
            Self::UpdateNotPermittedSequenceState => {
                "Update_not_permitted_sequence_state".to_string()
            }
            Self::UpdateNotPermittedZkappUri => "Update_not_permitted_zkapp_uri".to_string(),
            Self::UpdateNotPermittedTokenSymbol => "Update_not_permitted_token_symbol".to_string(),
            Self::UpdateNotPermittedPermissions => "Update_not_permitted_permissions".to_string(),
            Self::UpdateNotPermittedNonce => "Update_not_permitted_nonce".to_string(),
            Self::UpdateNotPermittedVotingFor => "Update_not_permitted_voting_for".to_string(),
            Self::ZkappCommandReplayCheckFailed => "Zkapp_command_replay_check_failed".to_string(),
            Self::FeePayerNonceMustIncrease => "Fee_payer_nonce_must_increase".to_string(),
            Self::FeePayerMustBeSigned => "Fee_payer_must_be_signed".to_string(),
            Self::AccountBalancePreconditionUnsatisfied => {
                "Account_balance_precondition_unsatisfied".to_string()
            }
            Self::AccountNoncePreconditionUnsatisfied => {
                "Account_nonce_precondition_unsatisfied".to_string()
            }
            Self::AccountReceiptChainHashPreconditionUnsatisfied => {
                "Account_receipt_chain_hash_precondition_unsatisfied".to_string()
            }
            Self::AccountDelegatePreconditionUnsatisfied => {
                "Account_delegate_precondition_unsatisfied".to_string()
            }
            Self::AccountSequenceStatePreconditionUnsatisfied => {
                "Account_sequence_state_precondition_unsatisfied".to_string()
            }
            Self::AccountAppStatePreconditionUnsatisfied(i) => {
                format!("Account_app_state_{}_precondition_unsatisfied", i)
            }
            Self::AccountProvedStatePreconditionUnsatisfied => {
                "Account_proved_state_precondition_unsatisfied".to_string()
            }
            Self::AccountIsNewPreconditionUnsatisfied => {
                "Account_is_new_precondition_unsatisfied".to_string()
            }
            Self::ProtocolStatePreconditionUnsatisfied => {
                "Protocol_state_precondition_unsatisfied".to_string()
            }
            Self::IncorrectNonce => "Incorrect_nonce".to_string(),
            Self::InvalidFeeExcess => "Invalid_fee_excess".to_string(),
            Self::Cancelled => "Cancelled".to_string(),
        }
    }
}

pub fn single_failure() -> Vec<Vec<TransactionFailure>> {
    vec![vec![TransactionFailure::UpdateNotPermittedBalance]]
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/transaction_status.ml#L452
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Applied,
    Failed(Vec<Vec<TransactionFailure>>),
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/with_status.ml#L6
#[derive(Debug, Clone)]
pub struct WithStatus<T> {
    pub data: T,
    pub status: TransactionStatus,
}

impl<T> WithStatus<T> {
    pub fn map<F, R>(&self, fun: F) -> WithStatus<R>
    where
        F: Fn(&T) -> R,
    {
        WithStatus {
            data: fun(&self.data),
            status: self.status.clone(),
        }
    }

    pub fn into_map<F, R>(self, fun: F) -> WithStatus<R>
    where
        F: Fn(T) -> R,
    {
        WithStatus {
            data: fun(self.data),
            status: self.status,
        }
    }
}

pub trait GenericCommand {
    fn fee(&self) -> Fee;
    fn forget(&self) -> UserCommand;
}

pub trait GenericTransaction: Sized {
    fn is_fee_transfer(&self) -> bool;
    fn is_coinbase(&self) -> bool;
    fn is_command(&self) -> bool;
}

pub mod valid {
    use super::*;

    #[derive(Clone, Debug, Hash, PartialEq, Eq)]
    pub struct VerificationKeyHash(pub Fp);

    pub type SignedCommand = super::signed_command::SignedCommand;

    #[derive(Clone, Debug, PartialEq)]
    pub enum UserCommand {
        SignedCommand(Box<SignedCommand>),
        ZkAppCommand(Box<super::zkapp_command::valid::ZkAppCommand>),
    }

    impl UserCommand {
        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/user_command.ml#L277
        pub fn forget_check(&self) -> super::UserCommand {
            match self {
                UserCommand::SignedCommand(cmd) => super::UserCommand::SignedCommand(cmd.clone()),
                UserCommand::ZkAppCommand(cmd) => {
                    super::UserCommand::ZkAppCommand(Box::new(cmd.zkapp_command.clone()))
                }
            }
        }
    }

    impl GenericCommand for UserCommand {
        fn fee(&self) -> Fee {
            match self {
                UserCommand::SignedCommand(cmd) => cmd.fee(),
                UserCommand::ZkAppCommand(cmd) => cmd.zkapp_command.fee(),
            }
        }

        fn forget(&self) -> super::UserCommand {
            match self {
                UserCommand::SignedCommand(cmd) => super::UserCommand::SignedCommand(cmd.clone()),
                UserCommand::ZkAppCommand(cmd) => {
                    super::UserCommand::ZkAppCommand(Box::new(cmd.zkapp_command.clone()))
                }
            }
        }
    }

    impl GenericTransaction for Transaction {
        fn is_fee_transfer(&self) -> bool {
            matches!(self, Transaction::FeeTransfer(_))
        }
        fn is_coinbase(&self) -> bool {
            matches!(self, Transaction::Coinbase(_))
        }
        fn is_command(&self) -> bool {
            matches!(self, Transaction::Command(_))
        }
    }

    #[derive(Debug, derive_more::From)]
    pub enum Transaction {
        Command(UserCommand),
        FeeTransfer(super::FeeTransfer),
        Coinbase(super::Coinbase),
    }

    impl Transaction {
        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/transaction/transaction.ml#L61
        pub fn forget(&self) -> super::Transaction {
            match self {
                Transaction::Command(cmd) => super::Transaction::Command(cmd.forget_check()),
                Transaction::FeeTransfer(ft) => super::Transaction::FeeTransfer(ft.clone()),
                Transaction::Coinbase(cb) => super::Transaction::Coinbase(cb.clone()),
            }
        }
    }
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/fee_transfer.ml#L19
#[derive(Debug, Clone)]
pub struct SingleFeeTransfer {
    pub receiver_pk: CompressedPubKey,
    pub fee: Fee,
    pub fee_token: TokenId,
}

impl SingleFeeTransfer {
    pub fn receiver(&self) -> AccountId {
        AccountId {
            public_key: self.receiver_pk.clone(),
            token_id: self.fee_token.clone(),
        }
    }

    pub fn create(receiver_pk: CompressedPubKey, fee: Fee, fee_token: TokenId) -> Self {
        Self {
            receiver_pk,
            fee,
            fee_token,
        }
    }
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/fee_transfer.ml#L68
#[derive(Debug, Clone)]
pub struct FeeTransfer(pub(super) OneOrTwo<SingleFeeTransfer>);

impl std::ops::Deref for FeeTransfer {
    type Target = OneOrTwo<SingleFeeTransfer>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FeeTransfer {
    pub fn fee_tokens(&self) -> impl Iterator<Item = &TokenId> {
        self.0.iter().map(|fee_transfer| &fee_transfer.fee_token)
    }

    pub fn receiver_pks(&self) -> impl Iterator<Item = &CompressedPubKey> {
        self.0.iter().map(|fee_transfer| &fee_transfer.receiver_pk)
    }

    pub fn receivers(&self) -> impl Iterator<Item = AccountId> + '_ {
        self.0.iter().map(|fee_transfer| AccountId {
            public_key: fee_transfer.receiver_pk.clone(),
            token_id: fee_transfer.fee_token.clone(),
        })
    }

    /// https://github.com/MinaProtocol/mina/blob/e5183ca1dde1c085b4c5d37d1d9987e24c294c32/src/lib/mina_base/fee_transfer.ml#L109
    pub fn fee_excess(&self) -> Result<FeeExcess, String> {
        let one_or_two = self.0.map(|SingleFeeTransfer { fee, fee_token, .. }| {
            (fee_token.clone(), Signed::<Fee>::of_unsigned(*fee).negate())
        });
        FeeExcess::of_one_or_two(one_or_two)
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/fee_transfer.ml#L84
    pub fn of_singles(singles: OneOrTwo<SingleFeeTransfer>) -> Result<Self, String> {
        match singles {
            OneOrTwo::One(a) => Ok(Self(OneOrTwo::One(a))),
            OneOrTwo::Two((one, two)) => {
                if one.fee_token == two.fee_token {
                    Ok(Self(OneOrTwo::Two((one, two))))
                } else {
                    // Necessary invariant for the transaction snark: we should never have
                    // fee excesses in multiple tokens simultaneously.
                    return Err(format!(
                        "Cannot combine single fee transfers with incompatible tokens: {:?} <> {:?}",
                        one, two
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CoinbaseFeeTransfer {
    pub receiver_pk: CompressedPubKey,
    pub fee: Fee,
}

impl CoinbaseFeeTransfer {
    pub fn create(receiver_pk: CompressedPubKey, fee: Fee) -> Self {
        Self { receiver_pk, fee }
    }

    pub fn receiver(&self) -> AccountId {
        AccountId {
            public_key: self.receiver_pk.clone(),
            token_id: TokenId::default(),
        }
    }
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/coinbase.ml#L17
#[derive(Debug, Clone)]
pub struct Coinbase {
    pub receiver: CompressedPubKey,
    pub amount: Amount,
    pub fee_transfer: Option<CoinbaseFeeTransfer>,
}

impl Coinbase {
    fn is_valid(&self) -> bool {
        match &self.fee_transfer {
            None => true,
            Some(CoinbaseFeeTransfer { fee, .. }) => Amount::of_fee(fee) <= self.amount,
        }
    }

    pub fn create(
        amount: Amount,
        receiver: CompressedPubKey,
        fee_transfer: Option<CoinbaseFeeTransfer>,
    ) -> Result<Coinbase, String> {
        let mut this = Self {
            receiver: receiver.clone(),
            amount,
            fee_transfer,
        };

        if this.is_valid() {
            let adjusted_fee_transfer = this.fee_transfer.as_ref().and_then(|ft| {
                if receiver != ft.receiver_pk {
                    Some(ft.clone())
                } else {
                    None
                }
            });
            this.fee_transfer = adjusted_fee_transfer;
            Ok(this)
        } else {
            Err("Coinbase.create: invalid coinbase".to_string())
        }
    }

    fn expected_supply_increase(&self) -> Result<Amount, String> {
        let Self {
            amount,
            fee_transfer,
            ..
        } = self;

        match fee_transfer {
            None => Ok(*amount),
            Some(CoinbaseFeeTransfer { fee, .. }) => amount
                .checked_sub(&Amount::of_fee(fee))
                .ok_or_else(|| "Coinbase underflow".to_string()),
        }
    }

    pub fn fee_excess(&self) -> Result<FeeExcess, String> {
        self.expected_supply_increase().map(|_| FeeExcess::empty())
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/coinbase.ml#L39
    pub fn receiver(&self) -> AccountId {
        AccountId::new(self.receiver.clone(), TokenId::default())
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/coinbase.ml#L51
    pub fn accounts_accessed(&self) -> Vec<AccountId> {
        let mut ids = Vec::with_capacity(2);

        ids.push(self.receiver());

        if let Some(fee_transfer) = self.fee_transfer.as_ref() {
            ids.push(fee_transfer.receiver());
        };

        ids
    }
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signature.mli#L11
#[derive(Clone, PartialEq, Eq)]
pub struct Signature(pub(super) Fp, pub(super) Fp); // TODO: Not sure if it's correct

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Signature({:?},{:?})", self.0, self.1))
    }
}

#[derive(Clone, PartialEq)]
pub struct Memo(pub [u8; 34]);

impl std::fmt::Debug for Memo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = String::from_utf8_lossy(&self.0);
        f.write_fmt(format_args!("{:?}", s))
    }
}

impl Memo {
    pub fn hash(&self) -> Fp {
        todo!()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// https://github.com/MinaProtocol/mina/blob/3a78f0e0c1343d14e2729c8b00205baa2ec70c93/src/lib/mina_base/signed_command_memo.ml#L151
    pub fn dummy() -> Self {
        // TODO
        Self([0; 34])
    }

    pub fn with_number(number: usize) -> Self {
        let s = format!("{:034}", number);
        assert_eq!(s.len(), 34);
        Self(s.into_bytes().try_into().unwrap())
    }
}

impl Signature {
    pub fn dummy() -> Self {
        Self(Fp::one(), Fp::one())
    }
}

pub mod signed_command {
    use crate::{decompress_pk, scan_state::currency::Slot, AccountId};

    use super::*;

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.ml#L75
    #[derive(Debug, Clone, PartialEq)]
    pub struct Common {
        pub fee: Fee,
        pub fee_payer_pk: CompressedPubKey,
        pub nonce: Nonce,
        pub valid_until: Slot,
        pub memo: Memo,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/payment_payload.ml#L40
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PaymentPayload {
        pub source_pk: CompressedPubKey,
        pub receiver_pk: CompressedPubKey,
        pub amount: Amount,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/stake_delegation.ml#L11
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum StakeDelegationPayload {
        SetDelegate {
            delegator: CompressedPubKey,
            new_delegate: CompressedPubKey,
        },
    }

    impl StakeDelegationPayload {
        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/stake_delegation.ml#L30
        pub fn source(&self) -> AccountId {
            let Self::SetDelegate { delegator, .. } = self;
            AccountId::new(delegator.clone(), TokenId::default())
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/stake_delegation.ml#L28
        pub fn source_pk(&self) -> &CompressedPubKey {
            let Self::SetDelegate { delegator, .. } = self;
            delegator
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/stake_delegation.ml#L24
        pub fn receiver(&self) -> AccountId {
            let Self::SetDelegate { new_delegate, .. } = self;
            AccountId::new(new_delegate.clone(), TokenId::default())
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/stake_delegation.ml#L22
        pub fn receiver_pk(&self) -> &CompressedPubKey {
            let Self::SetDelegate { new_delegate, .. } = self;
            new_delegate
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.mli#L24
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Body {
        Payment(PaymentPayload),
        StakeDelegation(StakeDelegationPayload),
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.mli#L165
    #[derive(Debug, Clone, PartialEq)]
    pub struct SignedCommandPayload {
        pub common: Common,
        pub body: Body,
    }

    impl SignedCommandPayload {
        pub fn create(
            fee: Fee,
            fee_payer_pk: CompressedPubKey,
            nonce: Nonce,
            valid_until: Option<Slot>,
            memo: Memo,
            body: Body,
        ) -> Self {
            Self {
                common: Common {
                    fee,
                    fee_payer_pk,
                    nonce,
                    valid_until: valid_until.unwrap_or_else(Slot::max),
                    memo,
                },
                body,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct SignedCommand {
        pub payload: SignedCommandPayload,
        pub signer: CompressedPubKey, // TODO: This should be a `mina_signer::PubKey`
        pub signature: Signature,
    }

    impl SignedCommand {
        pub fn valid_until(&self) -> Slot {
            self.payload.common.valid_until
        }

        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.ml#L322
        pub fn fee_payer(&self) -> AccountId {
            let public_key = self.payload.common.fee_payer_pk.clone();
            AccountId::new(public_key, TokenId::default())
        }

        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.ml#L320
        pub fn fee_payer_pk(&self) -> &CompressedPubKey {
            &self.payload.common.fee_payer_pk
        }

        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.ml#L318
        pub fn fee_token(&self) -> TokenId {
            TokenId::default()
        }

        pub fn fee(&self) -> Fee {
            self.payload.common.fee
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command_payload.ml#L243
        pub fn source(&self) -> AccountId {
            match &self.payload.body {
                Body::Payment(payload) => {
                    AccountId::new(payload.source_pk.clone(), TokenId::default())
                }
                Body::StakeDelegation(payload) => payload.source(),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command_payload.ml#L227
        pub fn source_pk(&self) -> &CompressedPubKey {
            match &self.payload.body {
                Body::Payment(payload) => &payload.source_pk,
                Body::StakeDelegation(payload) => payload.source_pk(),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command_payload.ml#L250
        pub fn receiver(&self) -> AccountId {
            match &self.payload.body {
                Body::Payment(payload) => {
                    AccountId::new(payload.receiver_pk.clone(), TokenId::default())
                }
                Body::StakeDelegation(payload) => payload.receiver(),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command_payload.ml#L234
        pub fn receiver_pk(&self) -> &CompressedPubKey {
            match &self.payload.body {
                Body::Payment(payload) => &payload.receiver_pk,
                Body::StakeDelegation(payload) => payload.receiver_pk(),
            }
        }

        pub fn nonce(&self) -> Nonce {
            self.payload.common.nonce
        }

        pub fn fee_excess(&self) -> FeeExcess {
            FeeExcess::of_single((self.fee_token(), Signed::<Fee>::of_unsigned(self.fee())))
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command_payload.ml#L354
        pub fn accounts_accessed(&self, status: TransactionStatus) -> Vec<AccountId> {
            use TransactionStatus::*;

            match status {
                Applied => {
                    vec![self.fee_payer(), self.source(), self.receiver()]
                }
                Failed(_) => vec![self.fee_payer()],
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command.ml#L401
        pub fn public_keys(&self) -> [&CompressedPubKey; 3] {
            [self.fee_payer_pk(), self.source_pk(), self.receiver_pk()]
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command.ml#L407
        pub fn check_valid_keys(&self) -> bool {
            self.public_keys()
                .into_iter()
                .all(|pk| decompress_pk(pk).is_some())
        }
    }
}

pub mod zkapp_command {
    use ark_ff::Zero;
    use mina_p2p_messages::v2::{
        MinaBaseAccountUpdateTWireStableV1,
        MinaBaseZkappCommandTStableV1WireStableV1AccountUpdatesA,
    };
    use static_assertions::assert_eq_size_val;

    use crate::{
        account, hash_noinputs, hash_with_kimchi,
        scan_state::{
            conv::AsAccountUpdateWithHash,
            currency::{Balance, BlockTime, Length, MinMax, Signed, Slot},
        },
        AuthRequired, Inputs, MyCow, Permissions, ToInputs, TokenSymbol, VerificationKey,
        VotingFor, ZkAppUri,
    };

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub struct Event(pub Vec<Fp>);

    impl Event {
        pub fn hash(&self) -> Fp {
            hash_with_kimchi("MinaZkappEvent", &self.0[..])
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L834
    #[derive(Debug, Clone, PartialEq)]
    pub struct Events(pub Vec<Event>);

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L155
    #[derive(Debug, Clone, PartialEq)]
    pub struct SequenceEvents(pub Vec<Event>);

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L23
    trait MakeEvents {
        const SALT_PHRASE: &'static str;
        const HASH_PREFIX: &'static str;
        const DERIVER_NAME: (); // Unused here for now

        fn events(&self) -> &[Event];
    }

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L100
    impl MakeEvents for Events {
        const SALT_PHRASE: &'static str = "MinaZkappEventsEmpty";
        const HASH_PREFIX: &'static str = "MinaZkappEvents";
        const DERIVER_NAME: () = ();
        fn events(&self) -> &[Event] {
            self.0.as_slice()
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L156
    impl MakeEvents for SequenceEvents {
        const SALT_PHRASE: &'static str = "MinaZkappSequenceEmpty";
        const HASH_PREFIX: &'static str = "MinaZkappSeqEvents";
        const DERIVER_NAME: () = ();
        fn events(&self) -> &[Event] {
            self.0.as_slice()
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L52
    fn events_to_inputs<E>(e: &E, inputs: &mut Inputs)
    where
        E: MakeEvents,
    {
        let init = hash_noinputs(E::SALT_PHRASE);

        let field = e.events().iter().rfold(init, |accum, elem| {
            hash_with_kimchi(E::HASH_PREFIX, &[accum, elem.hash()])
        });

        inputs.append_field(field);
    }

    impl ToInputs for Events {
        fn to_inputs(&self, inputs: &mut Inputs) {
            events_to_inputs(self, inputs);
        }
    }

    impl ToInputs for SequenceEvents {
        fn to_inputs(&self, inputs: &mut Inputs) {
            events_to_inputs(self, inputs);
        }
    }

    /// Note: It's a different one than in the normal `Account`
    ///
    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L163
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Timing {
        pub initial_minimum_balance: Balance,
        pub cliff_time: Slot,
        pub cliff_amount: Amount,
        pub vesting_period: Slot,
        pub vesting_increment: Amount,
    }

    impl Timing {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L208
        fn dummy() -> Self {
            Self {
                initial_minimum_balance: Balance::zero(),
                cliff_time: Slot::zero(),
                cliff_amount: Amount::zero(),
                vesting_period: Slot::zero(),
                vesting_increment: Amount::zero(),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/transaction_logic/mina_transaction_logic.ml#L1278
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L228
        pub fn of_account_timing(timing: crate::account::Timing) -> Option<Self> {
            match timing {
                crate::Timing::Untimed => None,
                crate::Timing::Timed {
                    initial_minimum_balance,
                    cliff_time,
                    cliff_amount,
                    vesting_period,
                    vesting_increment,
                } => Some(Self {
                    initial_minimum_balance,
                    cliff_time,
                    cliff_amount,
                    vesting_period,
                    vesting_increment,
                }),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L219
        pub fn to_account_timing(self) -> crate::account::Timing {
            let Self {
                initial_minimum_balance,
                cliff_time,
                cliff_amount,
                vesting_period,
                vesting_increment,
            } = self;

            crate::account::Timing::Timed {
                initial_minimum_balance,
                cliff_time,
                cliff_amount,
                vesting_period,
                vesting_increment,
            }
        }
    }

    impl ToInputs for Timing {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L199
        fn to_inputs(&self, inputs: &mut Inputs) {
            let Timing {
                initial_minimum_balance,
                cliff_time,
                cliff_amount,
                vesting_period,
                vesting_increment,
            } = self;

            inputs.append_u64(initial_minimum_balance.as_u64());
            inputs.append_u32(cliff_time.as_u32());
            inputs.append_u64(cliff_amount.as_u64());
            inputs.append_u32(vesting_period.as_u32());
            inputs.append_u64(vesting_increment.as_u64());
        }
    }

    impl Events {
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn push_event(acc: Fp, event: Event) -> Fp {
            hash_with_kimchi("MinaZkappEvents", &[acc, event.hash()])
        }

        pub fn push_events(&self, acc: Fp) -> Fp {
            let hash = self
                .0
                .iter()
                .rfold(hash_noinputs("MinaZkappEventsEmpty"), |acc, e| {
                    Self::push_event(acc, e.clone())
                });
            hash_with_kimchi("MinaZkappEvents", &[acc, hash])
        }
    }

    impl SequenceEvents {
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn push_event(acc: Fp, event: Event) -> Fp {
            hash_with_kimchi("MinaZkappSeqEvents", &[acc, event.hash()])
        }

        pub fn push_events(&self, acc: Fp) -> Fp {
            let hash = self
                .0
                .iter()
                .rfold(hash_noinputs("MinaZkappSequenceEmpty"), |acc, e| {
                    Self::push_event(acc, e.clone())
                });
            hash_with_kimchi("MinaZkappSeqEvents", &[acc, hash])
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_basic.ml#L100
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SetOrKeep<T: Clone> {
        Set(T),
        Keep,
    }

    impl<T: Clone> SetOrKeep<T> {
        fn map<'a, F, U>(&'a self, fun: F) -> SetOrKeep<U>
        where
            F: FnOnce(&'a T) -> U,
            U: Clone,
        {
            match self {
                SetOrKeep::Set(v) => SetOrKeep::Set(fun(v)),
                SetOrKeep::Keep => SetOrKeep::Keep,
            }
        }

        pub fn into_map<F, U>(self, fun: F) -> SetOrKeep<U>
        where
            F: FnOnce(T) -> U,
            U: Clone,
        {
            match self {
                SetOrKeep::Set(v) => SetOrKeep::Set(fun(v)),
                SetOrKeep::Keep => SetOrKeep::Keep,
            }
        }

        pub fn set_or_keep(&self, x: T) -> T {
            match self {
                Self::Set(data) => data.clone(),
                Self::Keep => x,
            }
        }

        pub fn is_keep(&self) -> bool {
            match self {
                Self::Keep => true,
                Self::Set(_) => false,
            }
        }

        pub fn is_set(&self) -> bool {
            !self.is_keep()
        }
    }

    impl<T, F> ToInputs for (&SetOrKeep<T>, F)
    where
        T: ToInputs,
        T: Clone,
        F: Fn() -> T,
    {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_basic.ml#L223
        fn to_inputs(&self, inputs: &mut Inputs) {
            let (set_or_keep, default_fn) = self;

            match set_or_keep {
                SetOrKeep::Set(this) => {
                    inputs.append_bool(true);
                    this.to_inputs(inputs);
                }
                SetOrKeep::Keep => {
                    inputs.append_bool(false);
                    let default = default_fn();
                    default.to_inputs(inputs);
                }
            };
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WithHash<T> {
        pub data: T,
        pub hash: Fp,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L319
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Update {
        pub app_state: [SetOrKeep<Fp>; 8],
        pub delegate: SetOrKeep<CompressedPubKey>,
        pub verification_key: SetOrKeep<WithHash<VerificationKey>>,
        pub permissions: SetOrKeep<Permissions<AuthRequired>>,
        pub zkapp_uri: SetOrKeep<ZkAppUri>,
        pub token_symbol: SetOrKeep<TokenSymbol>,
        pub timing: SetOrKeep<Timing>,
        pub voting_for: SetOrKeep<VotingFor>,
    }

    pub trait Check {
        type A;
        type B;

        fn check(&self, label: String, x: Self::B) -> Result<(), String>;
    }

    impl<T> Check for T
    where
        T: Eq,
    {
        type A = T;
        type B = T;

        fn check(&self, label: String, rhs: Self::B) -> Result<(), String> {
            if *self == rhs {
                Ok(())
            } else {
                Err(format!("Equality check failed: {}", label))
            }
        }
    }

    // TODO: This could be std::ops::Range ?
    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L23
    #[derive(Debug, Clone, PartialEq)]
    pub struct ClosedInterval<T> {
        pub lower: T,
        pub upper: T,
    }

    impl<T> ClosedInterval<T>
    where
        T: MinMax,
    {
        fn min_max() -> Self {
            Self {
                lower: T::min(),
                upper: T::max(),
            }
        }
    }

    impl<T> ToInputs for ClosedInterval<T>
    where
        T: ToInputs,
    {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L37
        fn to_inputs(&self, inputs: &mut Inputs) {
            let ClosedInterval { lower, upper } = self;

            lower.to_inputs(inputs);
            upper.to_inputs(inputs);
        }
    }

    impl<T> Check for ClosedInterval<T>
    where
        T: PartialOrd,
    {
        type A = ClosedInterval<T>;
        type B = T;

        fn check(&self, label: String, rhs: Self::B) -> Result<(), String> {
            if rhs >= self.lower && rhs <= self.upper {
                Ok(())
            } else {
                Err(format!("Bounds check failed: {}", label))
            }
        }
    }

    impl<T> ClosedInterval<T>
    where
        T: PartialOrd,
    {
        pub fn is_constant(&self) -> bool {
            self.lower == self.upper
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_basic.ml#L232
    #[derive(Debug, Clone, PartialEq)]
    pub enum OrIgnore<T> {
        Check(T),
        Ignore,
    }

    impl<T, F> ToInputs for (&OrIgnore<T>, F)
    where
        T: ToInputs,
        F: Fn() -> T,
    {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L414
        fn to_inputs(&self, inputs: &mut Inputs) {
            let (or_ignore, default_fn) = self;

            match or_ignore {
                OrIgnore::Check(this) => {
                    inputs.append_bool(true);
                    this.to_inputs(inputs);
                }
                OrIgnore::Ignore => {
                    inputs.append_bool(false);
                    let default = default_fn();
                    default.to_inputs(inputs);
                }
            };
        }
    }

    impl<T> OrIgnore<T>
    where
        T: Check<A = T>,
    {
        fn check(&self, label: String, rhs: T::B) -> Result<(), String> {
            match self {
                Self::Ignore => Ok(()),
                Self::Check(t) => t.check(label, rhs),
            }
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L439
    pub type Hash<T> = OrIgnore<T>;

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L298
    pub type EqData<T> = OrIgnore<T>;

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L178
    pub type Numeric<T> = OrIgnore<ClosedInterval<T>>;

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/epoch_ledger.ml#L9
    #[derive(Debug, Clone, PartialEq)]
    pub struct EpochLedger {
        pub hash: Hash<Fp>,
        pub total_currency: Numeric<Amount>,
    }

    impl EpochLedger {
        pub fn epoch_ledger(&self, t: protocol_state::EpochLedger) -> Result<(), String> {
            self.hash.check("epoch_ledger_hash".to_string(), t.hash)?;
            self.total_currency
                .check("epoch_ledger_total_currency".to_string(), t.total_currency)
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L797
    #[derive(Debug, Clone, PartialEq)]
    pub struct EpochData {
        pub(crate) ledger: EpochLedger,
        pub seed: Hash<Fp>,
        pub start_checkpoint: Hash<Fp>,
        pub lock_checkpoint: Hash<Fp>,
        pub epoch_length: Numeric<Length>,
    }

    impl ToInputs for EpochData {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L875
        fn to_inputs(&self, inputs: &mut Inputs) {
            let EpochData {
                ledger,
                seed,
                start_checkpoint,
                lock_checkpoint,
                epoch_length,
            } = self;

            {
                let EpochLedger {
                    hash,
                    total_currency,
                } = ledger;

                inputs.append(&(hash, Fp::zero));
                inputs.append(&(total_currency, ClosedInterval::min_max));
            }

            inputs.append(&(seed, Fp::zero));
            inputs.append(&(start_checkpoint, Fp::zero));
            inputs.append(&(lock_checkpoint, Fp::zero));
            inputs.append(&(epoch_length, ClosedInterval::min_max));
        }
    }

    impl EpochData {
        pub fn epoch_data(&self, label: &str, t: protocol_state::EpochData) -> Result<(), String> {
            self.ledger.epoch_ledger(t.ledger)?;
            // ignore seed
            self.start_checkpoint.check(
                format!("{}_{}", label, "start_checkpoint"),
                t.start_checkpoint,
            )?;
            self.lock_checkpoint.check(
                format!("{}_{}", label, "lock_checkpoint"),
                t.lock_checkpoint,
            )?;
            return self
                .epoch_length
                .check(format!("{}_{}", label, "epoch_length"), t.epoch_length);
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L977
    #[derive(Debug, Clone, PartialEq)]
    pub struct ZkAppPreconditions {
        pub snarked_ledger_hash: Hash<Fp>,
        pub timestamp: Numeric<BlockTime>,
        pub blockchain_length: Numeric<Length>,
        pub min_window_density: Numeric<Length>,
        pub last_vrf_output: (), // It's not defined in OCAml
        pub total_currency: Numeric<Amount>,
        pub global_slot_since_hard_fork: Numeric<Slot>,
        pub global_slot_since_genesis: Numeric<Slot>,
        pub staking_epoch_data: EpochData,
        pub next_epoch_data: EpochData,
    }

    impl ZkAppPreconditions {
        pub fn check(&self, s: ProtocolStateView) -> Result<(), String> {
            self.snarked_ledger_hash
                .check("snarker_ledger_hash".to_string(), s.snarked_ledger_hash)?;
            self.timestamp.check("timestamp".to_string(), s.timestamp)?;
            self.min_window_density
                .check("min_window_density".to_string(), s.min_window_density)?;
            self.total_currency
                .check("total_currency".to_string(), s.total_currency)?;
            self.global_slot_since_hard_fork.check(
                "curr_global_slot".to_string(),
                s.global_slot_since_hard_fork,
            )?;
            self.global_slot_since_genesis.check(
                "global_slot_since_genesis".to_string(),
                s.global_slot_since_genesis,
            )?;
            self.staking_epoch_data
                .epoch_data("stacking_epoch_data", s.staking_epoch_data)?;
            self.next_epoch_data
                .epoch_data("next_epoch_data", s.next_epoch_data)
        }
    }

    impl ToInputs for ZkAppPreconditions {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L1052
        fn to_inputs(&self, inputs: &mut Inputs) {
            let ZkAppPreconditions {
                snarked_ledger_hash,
                timestamp,
                blockchain_length,
                min_window_density,
                last_vrf_output,
                total_currency,
                global_slot_since_hard_fork,
                global_slot_since_genesis,
                staking_epoch_data,
                next_epoch_data,
            } = &self;

            assert_eq_size_val!(*last_vrf_output, ());

            inputs.append(&(snarked_ledger_hash, Fp::zero));
            inputs.append(&(timestamp, ClosedInterval::min_max));

            inputs.append(&(blockchain_length, ClosedInterval::min_max));
            inputs.append(&(min_window_density, ClosedInterval::min_max));
            inputs.append(&(total_currency, ClosedInterval::min_max));
            inputs.append(&(global_slot_since_hard_fork, ClosedInterval::min_max));
            inputs.append(&(global_slot_since_genesis, ClosedInterval::min_max));

            inputs.append(staking_epoch_data);
            inputs.append(next_epoch_data);
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_precondition.ml#L478
    #[derive(Debug, Clone, PartialEq)]
    pub struct Account {
        pub balance: Numeric<Balance>,
        pub nonce: Numeric<Nonce>,
        pub receipt_chain_hash: Hash<Fp>, // TODO: Should be type `ReceiptChainHash`
        pub delegate: EqData<CompressedPubKey>,
        pub state: [EqData<Fp>; 8],
        pub sequence_state: EqData<Fp>,
        pub proved_state: EqData<bool>,
        pub is_new: EqData<bool>,
    }

    impl Account {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L525
        pub fn accept() -> Self {
            Self {
                balance: Numeric::Ignore,
                nonce: Numeric::Ignore,
                receipt_chain_hash: Hash::Ignore,
                delegate: EqData::Ignore,
                state: std::array::from_fn(|_| EqData::Ignore),
                sequence_state: EqData::Ignore,
                proved_state: EqData::Ignore,
                is_new: EqData::Ignore,
            }
        }
    }

    impl Account {
        pub fn check<F>(&self, new_account: bool, mut check: F, a: account::Account)
        where
            F: FnMut(TransactionFailure, bool),
        {
            self.checks(new_account, a)
                .iter()
                .for_each(|(failure, res)| check(failure.clone(), res.is_ok()))
        }

        fn checks(
            &self,
            _new_account: bool,
            a: account::Account,
        ) -> Vec<(TransactionFailure, Result<(), String>)> {
            vec![
                (
                    TransactionFailure::AccountBalancePreconditionUnsatisfied,
                    self.balance.check("balance".to_string(), a.balance),
                ),
                (
                    TransactionFailure::AccountNoncePreconditionUnsatisfied,
                    self.nonce.check("nonce".to_string(), a.nonce),
                ),
                (
                    TransactionFailure::AccountReceiptChainHashPreconditionUnsatisfied,
                    self.receipt_chain_hash
                        .check("receipt_chain_hash".to_string(), a.receipt_chain_hash.0),
                ),
                (
                    TransactionFailure::AccountDelegatePreconditionUnsatisfied,
                    self.delegate
                        .check("delegate".to_string(), a.delegate.unwrap()), // TODO: handle None case?
                ),
            ]
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L613
    #[derive(Debug, Clone, PartialEq)]
    pub enum AccountPreconditions {
        Full(Box<Account>),
        Nonce(Nonce),
        Accept,
    }

    impl ToInputs for AccountPreconditions {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L635
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_precondition.ml#L568
        fn to_inputs(&self, inputs: &mut Inputs) {
            let account = match self {
                AccountPreconditions::Full(account) => MyCow::Borrow(&**account),
                AccountPreconditions::Nonce(nonce) => {
                    let mut account = Account::accept();
                    account.nonce = Numeric::Check(ClosedInterval {
                        lower: *nonce,
                        upper: *nonce,
                    });
                    MyCow::Own(account)
                }
                AccountPreconditions::Accept => MyCow::Own(Account::accept()),
            };

            let Account {
                balance,
                nonce,
                receipt_chain_hash,
                delegate,
                state,
                sequence_state,
                proved_state,
                is_new,
            } = account.as_ref();

            inputs.append(&(balance, ClosedInterval::min_max));
            inputs.append(&(nonce, ClosedInterval::min_max));
            inputs.append(&(receipt_chain_hash, Fp::zero));
            inputs.append(&(delegate, CompressedPubKey::empty));

            state.iter().for_each(|s| {
                inputs.append(&(s, Fp::zero));
            });

            // https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_account.ml#L168
            inputs.append(&(sequence_state, || {
                hash_noinputs("MinaZkappSequenceStateEmptyElt")
            }));

            inputs.append(&(proved_state, || false));
            inputs.append(&(is_new, || false));
        }
    }

    impl AccountPreconditions {
        pub fn nonce(&self) -> Numeric<Nonce> {
            match self {
                Self::Full(account) => account.nonce.clone(),
                Self::Nonce(nonce) => Numeric::Check(ClosedInterval {
                    lower: *nonce,
                    upper: *nonce,
                }),
                Self::Accept => Numeric::Ignore,
            }
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L758
    #[derive(Debug, Clone, PartialEq)]
    pub struct Preconditions {
        pub(crate) network: ZkAppPreconditions,
        pub account: AccountPreconditions,
    }

    impl ToInputs for Preconditions {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L776
        fn to_inputs(&self, inputs: &mut Inputs) {
            let Self { network, account } = self;
            network.to_inputs(inputs);
            account.to_inputs(inputs);
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L27
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AuthorizationKind {
        NoneGiven,
        Signature,
        Proof,
    }

    impl ToInputs for AuthorizationKind {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L110
        fn to_inputs(&self, inputs: &mut Inputs) {
            // bits: [is_signed, is_proved]
            let bits = match self {
                AuthorizationKind::NoneGiven => [false, false],
                AuthorizationKind::Signature => [true, false],
                AuthorizationKind::Proof => [false, true],
            };

            for bit in bits {
                inputs.append_bool(bit);
            }
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L955
    #[derive(Debug, Clone, PartialEq)]
    pub struct Body {
        pub public_key: CompressedPubKey,
        pub token_id: TokenId,
        pub update: Update,
        pub balance_change: Signed<Amount>,
        pub increment_nonce: bool,
        pub events: Events,
        pub sequence_events: SequenceEvents,
        pub call_data: Fp,
        pub preconditions: Preconditions,
        pub use_full_commitment: bool,
        pub caller: TokenId,
        pub authorization_kind: AuthorizationKind,
    }

    /// Notes:
    /// The type in OCaml is this one:
    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/pickles/proof.ml#L401
    ///
    /// For now we use the type from `mina_p2p_messages`, but we need to use our own.
    /// Lots of inner types are (BigInt, Bigint) which should be replaced with `Pallas<_>` etc.
    /// Also, in OCaml it has custom `{to/from}_binable` implementation.
    ///
    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/pickles/pickles_intf.ml#L316
    pub type SideLoadedProof = mina_p2p_messages::v2::PicklesProofProofsVerifiedMaxStableV2;

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/control.ml#L11
    #[derive(Debug, Clone, PartialEq)]
    pub enum Control {
        Proof(SideLoadedProof),
        Signature(Signature),
        NoneGiven,
    }

    pub struct CheckAuthorizationResult {
        pub proof_verifies: bool,
        pub signature_verifies: bool,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L1437
    #[derive(Debug, Clone, PartialEq)]
    pub struct AccountUpdate {
        pub body: Body,
        pub authorization: Control,
    }

    impl ToInputs for AccountUpdate {
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L1297
        fn to_inputs(&self, inputs: &mut Inputs) {
            // Only the body is used
            let Self {
                body,
                authorization: _,
            } = self;

            let Body {
                public_key,
                token_id,
                update,
                balance_change,
                increment_nonce,
                events,
                sequence_events,
                call_data,
                preconditions,
                use_full_commitment,
                caller,
                authorization_kind,
            } = body;

            inputs.append(public_key);
            inputs.append(token_id);

            // `Body::update`
            {
                let Update {
                    app_state,
                    delegate,
                    verification_key,
                    permissions,
                    zkapp_uri,
                    token_symbol,
                    timing,
                    voting_for,
                } = update;

                for state in app_state {
                    inputs.append(&(state, Fp::zero));
                }

                inputs.append(&(delegate, CompressedPubKey::empty));
                inputs.append(&(&verification_key.map(|w| w.hash), Fp::zero));
                inputs.append(&(permissions, Permissions::user_default));
                inputs.append(&(&zkapp_uri.map(Some), || Option::<&ZkAppUri>::None));
                inputs.append(&(token_symbol, TokenSymbol::default));
                inputs.append(&(timing, Timing::dummy));
                inputs.append(&(voting_for, VotingFor::dummy));
            }

            inputs.append(balance_change);
            inputs.append(increment_nonce);
            inputs.append(events);
            inputs.append(sequence_events);
            inputs.append(call_data);
            inputs.append(preconditions);
            inputs.append(use_full_commitment);
            inputs.append(caller);
            inputs.append(authorization_kind);
        }
    }

    impl AccountUpdate {
        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/account_update.ml#L1535
        pub fn account_id(&self) -> AccountId {
            AccountId::new(self.body.public_key.clone(), self.body.token_id.clone())
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/account_update.ml#L1327
        pub fn digest(&self) -> Fp {
            let mut inputs = Inputs::new();

            self.to_inputs(&mut inputs);
            hash_with_kimchi("MinaZkappBody", &inputs.to_fields())
        }

        pub fn timing(&self) -> SetOrKeep<Timing> {
            self.body.update.timing.clone()
        }

        pub fn caller(&self) -> TokenId {
            self.body.caller.clone()
        }

        pub fn public_key(&self) -> CompressedPubKey {
            self.body.public_key.clone()
        }

        pub fn token_id(&self) -> TokenId {
            self.body.token_id.clone()
        }

        pub fn increment_nonce(&self) -> bool {
            self.body.increment_nonce
        }

        // commitment and calls argument are ignored here, only used in the transaction snark
        pub fn check_authorization(
            &self,
            _commitment: Fp,
            _calls: CallForest<()>,
        ) -> CheckAuthorizationResult {
            match self.authorization {
                Control::Signature(_) => CheckAuthorizationResult {
                    proof_verifies: false,
                    signature_verifies: true,
                },
                Control::Proof(_) => CheckAuthorizationResult {
                    proof_verifies: true,
                    signature_verifies: false,
                },
                Control::NoneGiven => CheckAuthorizationResult {
                    proof_verifies: false,
                    signature_verifies: false,
                },
            }
        }

        pub fn permissions(&self) -> SetOrKeep<Permissions<AuthRequired>> {
            self.body.update.permissions.clone()
        }

        pub fn app_state(&self) -> [SetOrKeep<Fp>; 8] {
            self.body.update.app_state.clone()
        }

        pub fn zkapp_uri(&self) -> SetOrKeep<ZkAppUri> {
            self.body.update.zkapp_uri.clone()
        }

        /*
        pub fn token_symbol(&self) -> SetOrKeep<[u8; 6]> {
            self.body.update.token_symbol.clone()
        }
        */

        pub fn token_symbol(&self) -> SetOrKeep<TokenSymbol> {
            self.body.update.token_symbol.clone()
        }

        pub fn delegate(&self) -> SetOrKeep<CompressedPubKey> {
            self.body.update.delegate.clone()
        }

        pub fn voting_for(&self) -> SetOrKeep<VotingFor> {
            self.body.update.voting_for.clone()
        }

        pub fn verification_key(&self) -> SetOrKeep<VerificationKey> {
            self.body.update.verification_key.map(|vk| vk.data.clone())
        }

        pub fn sequence_events(&self) -> SequenceEvents {
            self.body.sequence_events.clone()
        }

        pub fn balance_change(&self) -> Signed<Amount> {
            self.body.balance_change.clone()
        }
        pub fn use_full_commitment(&self) -> bool {
            self.body.use_full_commitment
        }

        pub fn protocol_state_precondition(&self) -> ZkAppPreconditions {
            self.body.preconditions.network.clone()
        }

        pub fn account_precondition(&self) -> AccountPreconditions {
            self.body.preconditions.account.clone()
        }
    }

    // Digest.Account_update.Stable.V1.t = Fp
    // Digest.Forest.Stable.V1.t = Fp

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_command.ml#L49
    #[derive(Debug, Clone, PartialEq)]
    pub struct Tree<Data: Clone> {
        pub account_update: (AccountUpdate, Data),
        pub account_update_digest: Fp,
        pub calls: CallForest<Data>,
    }

    impl<Data: Clone> Tree<Data> {
        fn digest(&self) -> Fp {
            let stack_hash = match self.calls.0.first() {
                Some(e) => e.stack_hash,
                None => Fp::zero(),
            };

            // self.account_update_digest should have been updated in `CallForest::accumulate_hashes`
            assert_ne!(self.account_update_digest, Fp::zero());

            hash_with_kimchi(
                "MinaAcctUpdateNode",
                &[self.account_update_digest, stack_hash],
            )
        }

        fn fold<F>(&self, init: Vec<AccountId>, f: &mut F) -> Vec<AccountId>
        where
            F: FnMut(Vec<AccountId>, &(AccountUpdate, Data)) -> Vec<AccountId>,
        {
            self.calls.fold(f(init, &self.account_update), f)
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/with_stack_hash.ml#L6
    #[derive(Debug, Clone, PartialEq)]
    pub struct WithStackHash<Data: Clone> {
        pub elt: Tree<Data>,
        pub stack_hash: Fp,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_command.ml#L345
    #[derive(Debug, Clone, PartialEq)]
    pub struct CallForest<Data: Clone>(pub Vec<WithStackHash<Data>>);

    impl<Data: Clone> Default for CallForest<Data> {
        fn default() -> Self {
            Self::new()
        }
    }

    #[derive(Clone)]
    struct CallForestContext {
        caller: TokenId,
        this: TokenId,
    }

    impl<Data: Clone> CallForest<Data> {
        pub fn new() -> Self {
            Self(Vec::new())
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        // In OCaml push/pop to the head is cheap because they work with lists.
        // In Rust we use vectors so we will push/pop to the tail.
        // To work with the elements as if they were in the original order we need to iterate backwards
        pub fn iter(&self) -> impl Iterator<Item = &WithStackHash<Data>> {
            self.0.iter().rev()
        }

        pub fn hash(&self) -> Fp {
            if let Some(x) = self.0.last() {
                x.stack_hash
            } else {
                Fp::zero()
            }
        }

        fn cons_tree(&self, tree: Tree<Data>) -> Self {
            let stack_hash = hash_with_kimchi("MinaAcctUpdateCons", &[tree.digest(), self.hash()]);
            let node = WithStackHash::<Data> {
                elt: tree,
                stack_hash,
            };
            let mut forest = self.0.clone();
            forest.push(node);
            Self { 0: forest }
        }

        pub fn cons(
            &self,
            calls: Option<CallForest<Data>>,
            account_update: AccountUpdate,
            data: Data,
        ) -> Self {
            let tree = Tree::<Data> {
                account_update: (account_update.clone(), data),
                account_update_digest: account_update.digest(),
                calls: calls.unwrap_or_default(),
            };
            self.cons_tree(tree)
        }

        pub fn pop_exn(&self) -> (((AccountUpdate, Data), CallForest<Data>), CallForest<Data>) {
            let mut ret = self.0.clone();

            if let Some(node) = ret.pop() {
                ((node.elt.account_update, node.elt.calls), CallForest(ret))
            } else {
                panic!()
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/zkapp_command.ml#L68
        fn fold_impl<A, F>(&self, init: A, fun: &mut F) -> A
        where
            F: FnMut(A, &(AccountUpdate, Data)) -> A,
        {
            let mut accum = init;
            for elem in self.iter() {
                accum = fun(accum, &elem.elt.account_update);
                accum = elem.elt.calls.fold_impl(accum, fun);
            }
            accum
        }

        pub fn fold<A, F>(&self, init: A, mut fun: F) -> A
        where
            F: FnMut(A, &(AccountUpdate, Data)) -> A,
        {
            self.fold_impl(init, &mut fun)
        }

        fn map_to_impl<F, VK: Clone>(&self, fun: &F) -> CallForest<VK>
        where
            F: Fn(&(AccountUpdate, Data)) -> (AccountUpdate, VK),
        {
            CallForest::<VK>(
                self.iter()
                    .map(|item| WithStackHash::<VK> {
                        elt: Tree::<VK> {
                            account_update: fun(&item.elt.account_update),
                            account_update_digest: item.elt.account_update_digest,
                            calls: item.elt.calls.map_to_impl(fun),
                        },
                        stack_hash: item.stack_hash,
                    })
                    .collect(),
            )
        }

        pub fn map_to<F, VK: Clone>(&self, fun: F) -> CallForest<VK>
        where
            F: Fn(&(AccountUpdate, Data)) -> (AccountUpdate, VK),
        {
            self.map_to_impl(&fun)
        }

        fn add_callers_impl<F, Update>(
            &mut self,
            wired: &[Update],
            current_context: CallForestContext,
            account_update_id: &F,
        ) where
            Update: AsAccountUpdateWithHash,
            F: Fn(&MinaBaseAccountUpdateTWireStableV1) -> TokenId,
        {
            use mina_p2p_messages::v2::MinaBaseAccountUpdateCallTypeStableV1::{
                Call, DelegateCall,
            };

            assert_eq!(self.0.len(), wired.len());

            self.0.iter_mut().zip(wired).for_each(|(elem, wired)| {
                let WithStackHash {
                    elt:
                        Tree::<Data> {
                            account_update,
                            calls,
                            ..
                        },
                    ..
                } = elem;

                let child_context = match &wired.elt().account_update.body.caller {
                    DelegateCall => current_context.clone(),
                    Call => CallForestContext {
                        caller: current_context.this.clone(),
                        this: account_update_id(&wired.elt().account_update),
                    },
                };

                account_update.0.body.caller = child_context.caller.clone();
                calls.add_callers_impl(&wired.elt().calls, child_context, account_update_id);
            });
        }

        /// Delegate_call means, preserve the current caller.
        ///
        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L616
        pub fn add_callers<F>(
            &mut self,
            wired: &[MinaBaseZkappCommandTStableV1WireStableV1AccountUpdatesA],
            account_update_id: F,
        ) where
            F: Fn(&MinaBaseAccountUpdateTWireStableV1) -> TokenId,
        {
            let current_context = CallForestContext {
                caller: TokenId::default(),
                this: TokenId::default(),
            };

            self.add_callers_impl(wired, current_context, &account_update_id);
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L583
        pub fn accumulate_hashes<F>(&mut self, hash_account_update: &F)
        where
            F: Fn(&AccountUpdate) -> Fp,
        {
            /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L293
            fn cons(hash: Fp, h_tl: Fp) -> Fp {
                hash_with_kimchi("MinaAcctUpdateCons", &[hash, h_tl])
            }

            /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L561
            fn hash<T: Clone>(elem: Option<&WithStackHash<T>>) -> Fp {
                match elem {
                    Some(next) => next.stack_hash,
                    None => Fp::zero(),
                }
            }

            // We traverse the list in reverse here (to get same behavior as OCaml recursivity)
            // We use indexes to make the borrow checker happy

            for index in (0..self.0.len()).rev() {
                let elem = &mut self.0[index];
                let WithStackHash {
                    elt:
                        Tree::<Data> {
                            account_update,
                            account_update_digest,
                            calls,
                            ..
                        },
                    ..
                } = elem;

                calls.accumulate_hashes(hash_account_update);
                *account_update_digest = hash_account_update(&account_update.0);

                let node_hash = elem.elt.digest();
                let hash = hash(self.0.get(index + 1));

                self.0[index].stack_hash = cons(node_hash, hash);
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L672
        fn remove_callers_impl<Update>(
            &self,
            wired: &mut [Update],
            is_top_level: bool,
            parent_caller: &TokenId,
        ) where
            Update: AsAccountUpdateWithHash,
        {
            use mina_p2p_messages::v2::MinaBaseAccountUpdateCallTypeStableV1::{
                Call, DelegateCall,
            };

            let call_type = |account_caller: &TokenId| {
                if is_top_level {
                    Call
                } else if account_caller == parent_caller {
                    DelegateCall
                } else {
                    Call
                }
            };

            wired.iter_mut().zip(&self.0).for_each(|(wired, this)| {
                let WithStackHash {
                    elt:
                        Tree::<Data> {
                            account_update,
                            calls,
                            ..
                        },
                    ..
                } = this;

                let wired = wired.elt_mut();
                let account_caller = &account_update.0.body.caller;

                wired.account_update.body.caller = call_type(account_caller);

                calls.remove_callers_impl(&mut wired.calls, false, account_caller);
            });
        }

        fn remove_callers(
            &self,
            wired: &mut [MinaBaseZkappCommandTStableV1WireStableV1AccountUpdatesA],
        ) {
            self.remove_callers_impl(wired, true, &TokenId::default());
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L1079
        pub fn of_wire(
            &mut self,
            wired: &[MinaBaseZkappCommandTStableV1WireStableV1AccountUpdatesA],
        ) {
            self.add_callers(wired, |wired_update| {
                let public_key: CompressedPubKey = (&wired_update.body.public_key).into();
                let token_id: TokenId = (&*wired_update.body.token_id).into();

                AccountId::new(public_key, token_id).derive_token_id()
            });

            self.accumulate_hashes(&|account_update| account_update.digest());
        }

        /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/zkapp_command.ml#L1096
        pub fn to_wire(
            &self,
            wired: &mut [MinaBaseZkappCommandTStableV1WireStableV1AccountUpdatesA],
        ) {
            self.remove_callers(wired);
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L1081
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FeePayerBody {
        pub public_key: CompressedPubKey,
        pub fee: Fee,
        pub valid_until: Option<Slot>,
        pub nonce: Nonce,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/account_update.ml#L1484
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FeePayer {
        pub body: FeePayerBody,
        pub authorization: Signature,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/zkapp_command.ml#L959
    #[derive(Debug, Clone, PartialEq)]
    pub struct ZkAppCommand {
        pub fee_payer: FeePayer,
        pub account_updates: CallForest<()>,
        pub memo: Memo,
    }

    impl ZkAppCommand {
        pub fn fee_payer(&self) -> AccountId {
            let public_key = self.fee_payer.body.public_key.clone();
            AccountId::new(public_key, self.fee_token())
        }

        pub fn fee_token(&self) -> TokenId {
            TokenId::default()
        }

        pub fn fee(&self) -> Fee {
            self.fee_payer.body.fee
        }

        pub fn fee_excess(&self) -> FeeExcess {
            FeeExcess::of_single((self.fee_token(), Signed::<Fee>::of_unsigned(self.fee())))
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/zkapp_command.ml#L1241
        pub fn accounts_accessed(&self, status: TransactionStatus) -> Vec<AccountId> {
            use TransactionStatus::*;

            match status {
                Applied => {
                    let ids = self.account_updates.fold(
                        Vec::with_capacity(256),
                        |mut accum, (account_update, _)| {
                            accum.push(account_update.account_id());
                            accum
                        },
                    );
                    ids.iter().unique().rev().cloned().collect()
                }
                Failed(_) => vec![self.fee_payer()],
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/zkapp_command.ml#L1251
        pub fn accounts_referenced(&self) -> Vec<AccountId> {
            self.accounts_accessed(TransactionStatus::Applied)
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/zkapp_command.ml#L1346
        pub fn of_verifiable(verifiable: verifiable::ZkAppCommand) -> Self {
            Self {
                fee_payer: verifiable.fee_payer,
                account_updates: verifiable
                    .account_updates
                    .map_to(|(acc, _)| (acc.clone(), ())),
                memo: verifiable.memo,
            }
        }
    }

    pub mod verifiable {
        use super::*;
        use crate::VerificationKey;

        #[derive(Debug, Clone)]
        pub struct ZkAppCommand {
            pub fee_payer: FeePayer,
            pub account_updates: CallForest<Option<WithHash<VerificationKey>>>,
            pub memo: Memo,
        }
    }

    pub mod valid {
        use std::collections::HashMap;

        use crate::scan_state::transaction_logic::valid::VerificationKeyHash;

        use super::*;

        #[derive(Clone, Debug, PartialEq)]
        pub struct ZkAppCommand {
            pub zkapp_command: super::ZkAppCommand,
            pub verification_keys: Vec<(AccountId, VerificationKeyHash)>,
        }

        /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/zkapp_command.ml#L1486
        pub fn of_verifiable(cmd: verifiable::ZkAppCommand) -> Option<ZkAppCommand> {
            use AuthorizationKind as AK;
            use Control as C;

            let mut keys = HashMap::with_capacity(256);

            cmd.account_updates.fold(Some(()), |accum, (p, vk_opt)| {
                accum?;

                match (&p.authorization, &p.body.authorization_kind) {
                    (C::NoneGiven, AK::NoneGiven)
                    | (C::Proof(_), AK::Proof)
                    | (C::Signature(_), AK::Signature) => {}
                    _ => return None,
                }

                if let C::Proof(_) = &p.authorization {
                    let hash = vk_opt.as_ref()?.hash;
                    keys.insert(p.account_id(), VerificationKeyHash(hash));
                };
                Some(())
            })?;

            Some(ZkAppCommand {
                zkapp_command: super::ZkAppCommand::of_verifiable(cmd),
                verification_keys: keys.into_iter().collect(),
            })
        }
    }
}

pub mod verifiable {
    use super::*;

    #[derive(Debug)]
    pub enum UserCommand {
        SignedCommand(Box<signed_command::SignedCommand>),
        ZkAppCommand(Box<zkapp_command::verifiable::ZkAppCommand>),
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command.ml#L436
    pub fn check_only_for_signature(cmd: Box<signed_command::SignedCommand>) -> valid::UserCommand {
        // TODO implement actual verification
        // https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/signed_command.ml#L396

        valid::UserCommand::SignedCommand(cmd)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserCommand {
    SignedCommand(Box<signed_command::SignedCommand>),
    ZkAppCommand(Box<zkapp_command::ZkAppCommand>),
}

impl UserCommand {
    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/user_command.ml#L203
    pub fn accounts_accessed(&self, status: TransactionStatus) -> Vec<AccountId> {
        match self {
            UserCommand::SignedCommand(cmd) => cmd.accounts_accessed(status),
            UserCommand::ZkAppCommand(cmd) => cmd.accounts_accessed(status),
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/user_command.ml#L210
    pub fn accounts_referenced(&self) -> Vec<AccountId> {
        self.accounts_accessed(TransactionStatus::Applied)
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/mina_base/user_command.ml#L192
    pub fn fee(&self) -> Fee {
        match self {
            UserCommand::SignedCommand(cmd) => cmd.fee(),
            UserCommand::ZkAppCommand(cmd) => cmd.fee(),
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/3fe924c80a4d01f418b69f27398f5f93eb652514/src/lib/mina_base/user_command.ml#L162
    pub fn to_verifiable(&self, ledger: &impl BaseLedger) -> verifiable::UserCommand {
        let find_vk = |acc: &zkapp_command::AccountUpdate| -> Option<VerificationKey> {
            let account_id = acc.account_id();
            let addr = ledger.location_of_account(&account_id)?;
            let account = ledger.get(addr)?;
            account.zkapp.as_ref()?.verification_key.clone()
        };

        match self {
            UserCommand::SignedCommand(cmd) => verifiable::UserCommand::SignedCommand(cmd.clone()),
            UserCommand::ZkAppCommand(cmd) => {
                let zkapp_command::ZkAppCommand {
                    fee_payer,
                    account_updates,
                    memo,
                } = &**cmd;

                let zkapp = zkapp_command::verifiable::ZkAppCommand {
                    fee_payer: fee_payer.clone(),
                    account_updates: account_updates.map_to(|(account_update, _)| {
                        let vk_with_hash = find_vk(account_update).map(|vk| {
                            let hash = vk.hash();
                            WithHash { data: vk, hash }
                        });

                        (account_update.clone(), vk_with_hash)
                    }),
                    memo: memo.clone(),
                };

                verifiable::UserCommand::ZkAppCommand(Box::new(zkapp))
            }
        }
    }
}

impl GenericCommand for UserCommand {
    fn fee(&self) -> Fee {
        match self {
            UserCommand::SignedCommand(cmd) => cmd.fee(),
            UserCommand::ZkAppCommand(cmd) => cmd.fee(),
        }
    }

    fn forget(&self) -> UserCommand {
        self.clone()
    }
}

impl GenericTransaction for Transaction {
    fn is_fee_transfer(&self) -> bool {
        matches!(self, Transaction::FeeTransfer(_))
    }
    fn is_coinbase(&self) -> bool {
        matches!(self, Transaction::Coinbase(_))
    }
    fn is_command(&self) -> bool {
        matches!(self, Transaction::Command(_))
    }
}

#[derive(Debug, derive_more::From)]
pub enum Transaction {
    Command(UserCommand),
    FeeTransfer(FeeTransfer),
    Coinbase(Coinbase),
}

impl Transaction {
    pub fn fee_excess(&self) -> Result<FeeExcess, String> {
        use Transaction::*;
        use UserCommand::*;

        match self {
            Command(SignedCommand(cmd)) => Ok(cmd.fee_excess()),
            Command(ZkAppCommand(cmd)) => Ok(cmd.fee_excess()),
            FeeTransfer(ft) => ft.fee_excess(),
            Coinbase(cb) => cb.fee_excess(),
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/05c2f73d0f6e4f1341286843814ce02dcb3919e0/src/lib/transaction/transaction.ml#L85
    pub fn public_keys(&self) -> Vec<CompressedPubKey> {
        use Transaction::*;
        use UserCommand::*;

        let to_pks = |ids: Vec<AccountId>| ids.into_iter().map(|id| id.public_key).collect();

        match self {
            Command(SignedCommand(cmd)) => [cmd.fee_payer_pk(), cmd.source_pk(), cmd.receiver_pk()]
                .into_iter()
                .cloned()
                .collect(),
            Command(ZkAppCommand(cmd)) => to_pks(cmd.accounts_referenced()),
            FeeTransfer(ft) => ft.receiver_pks().cloned().collect(),
            Coinbase(cb) => to_pks(cb.accounts_accessed()),
        }
    }
}

pub mod transaction_applied {
    use crate::{Account, AccountId};

    use super::*;

    pub mod signed_command_applied {
        use super::*;

        #[derive(Debug, Clone)]
        pub struct Common {
            pub user_command: WithStatus<signed_command::SignedCommand>,
        }

        #[derive(Debug, Clone)]
        pub enum Body {
            Payments {
                new_accounts: Vec<AccountId>,
            },
            StakeDelegation {
                previous_delegate: Option<CompressedPubKey>,
            },
            Failed,
        }

        #[derive(Debug, Clone)]
        pub struct SignedCommandApplied {
            pub common: Common,
            pub body: Body,
        }
    }

    pub use signed_command_applied::SignedCommandApplied;

    impl SignedCommandApplied {
        pub fn new_accounts(&self) -> &[AccountId] {
            use signed_command_applied::Body::*;

            match &self.body {
                Payments { new_accounts } => new_accounts.as_slice(),
                StakeDelegation { .. } | Failed => &[],
            }
        }
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L65
    #[derive(Debug, Clone)]
    pub struct ZkappCommandApplied {
        pub accounts: Vec<(AccountId, Option<Account>)>,
        pub command: WithStatus<zkapp_command::ZkAppCommand>,
        pub new_accounts: Vec<AccountId>,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L82
    #[derive(Debug, Clone)]
    pub enum CommandApplied {
        SignedCommand(Box<SignedCommandApplied>),
        ZkappCommand(Box<ZkappCommandApplied>),
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L96
    #[derive(Debug, Clone)]
    pub struct FeeTransferApplied {
        pub fee_transfer: WithStatus<FeeTransfer>,
        pub new_accounts: Vec<AccountId>,
        pub burned_tokens: Amount,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L112
    #[derive(Debug, Clone)]
    pub struct CoinbaseApplied {
        pub coinbase: WithStatus<Coinbase>,
        pub new_accounts: Vec<AccountId>,
        pub burned_tokens: Amount,
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L142
    #[derive(Debug, Clone)]
    pub enum Varying {
        Command(CommandApplied),
        FeeTransfer(FeeTransferApplied),
        Coinbase(CoinbaseApplied),
    }

    /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L142
    #[derive(Debug, Clone)]
    pub struct TransactionApplied {
        pub previous_hash: Fp,
        pub varying: Varying,
    }

    impl TransactionApplied {
        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L639
        pub fn transaction(&self) -> WithStatus<Transaction> {
            use CommandApplied::*;
            use Varying::*;

            match &self.varying {
                Command(SignedCommand(cmd)) => cmd
                    .common
                    .user_command
                    .map(|c| Transaction::Command(UserCommand::SignedCommand(Box::new(c.clone())))),
                Command(ZkappCommand(cmd)) => cmd
                    .command
                    .map(|c| Transaction::Command(UserCommand::ZkAppCommand(Box::new(c.clone())))),
                FeeTransfer(f) => f.fee_transfer.map(|f| Transaction::FeeTransfer(f.clone())),
                Coinbase(c) => c.coinbase.map(|c| Transaction::Coinbase(c.clone())),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L662
        pub fn transaction_status(&self) -> &TransactionStatus {
            use CommandApplied::*;
            use Varying::*;

            match &self.varying {
                Command(SignedCommand(cmd)) => &cmd.common.user_command.status,
                Command(ZkappCommand(cmd)) => &cmd.command.status,
                FeeTransfer(f) => &f.fee_transfer.status,
                Coinbase(c) => &c.coinbase.status,
            }
        }

        pub fn burned_tokens(&self) -> Amount {
            match &self.varying {
                Varying::Command(_) => Amount::zero(),
                Varying::FeeTransfer(f) => f.burned_tokens,
                Varying::Coinbase(c) => c.burned_tokens,
            }
        }

        pub fn new_accounts(&self) -> &[AccountId] {
            use CommandApplied::*;
            use Varying::*;

            match &self.varying {
                Command(SignedCommand(cmd)) => cmd.new_accounts(),
                Command(ZkappCommand(cmd)) => cmd.new_accounts.as_slice(),
                FeeTransfer(f) => f.new_accounts.as_slice(),
                Coinbase(cb) => cb.new_accounts.as_slice(),
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/e5183ca1dde1c085b4c5d37d1d9987e24c294c32/src/lib/transaction_logic/mina_transaction_logic.ml#L176
        pub fn supply_increase(
            &self,
            constraint_constants: &ConstraintConstants,
        ) -> Result<Signed<Amount>, String> {
            let burned_tokens = Signed::<Amount>::of_unsigned(self.burned_tokens());

            let account_creation_fees = {
                let account_creation_fee_int = constraint_constants.account_creation_fee.as_u64();
                let num_accounts_created = self.new_accounts().len() as u64;

                // int type is OK, no danger of overflow
                let amount = account_creation_fee_int
                    .checked_mul(num_accounts_created)
                    .unwrap();
                Signed::<Amount>::of_unsigned(Amount::from_u64(amount))
            };

            let expected_supply_increase = match &self.varying {
                Varying::Coinbase(cb) => cb.coinbase.data.expected_supply_increase()?,
                _ => Amount::zero(),
            };
            let expected_supply_increase = Signed::<Amount>::of_unsigned(expected_supply_increase);

            // TODO: Make sure it's correct
            let total = [burned_tokens, account_creation_fees]
                .into_iter()
                .fold(Some(expected_supply_increase), |total, amt| {
                    amt.negate().add(&total?)
                });

            total.ok_or_else(|| "overflow".to_string())
        }
    }
}

pub mod transaction_witness {
    use mina_p2p_messages::v2::MinaStateProtocolStateBodyValueStableV2;

    use crate::scan_state::pending_coinbase::Stack;

    use super::*;

    #[derive(Debug)]
    pub struct TransactionWitness {
        pub transaction: Transaction,
        pub ledger: SparseLedger<AccountId, Account>,
        pub protocol_state_body: MinaStateProtocolStateBodyValueStableV2,
        pub init_stack: Stack,
        pub status: TransactionStatus,
    }
}

pub mod protocol_state {
    use mina_p2p_messages::v2::MinaStateProtocolStateValueStableV2;

    use super::*;

    #[derive(Debug, Clone)]
    pub struct EpochLedger {
        pub hash: Fp,
        pub total_currency: Amount,
    }

    #[derive(Debug, Clone)]
    pub struct EpochData {
        pub ledger: EpochLedger,
        pub seed: Fp,
        pub start_checkpoint: Fp,
        pub lock_checkpoint: Fp,
        pub epoch_length: Length,
    }

    #[derive(Debug, Clone)]
    pub struct ProtocolStateView {
        pub snarked_ledger_hash: Fp,
        pub timestamp: BlockTime,
        pub blockchain_length: Length,
        pub min_window_density: Length,
        pub last_vrf_output: (), // It's not defined in OCAml
        pub total_currency: Amount,
        pub global_slot_since_hard_fork: Slot,
        pub global_slot_since_genesis: Slot,
        pub staking_epoch_data: EpochData,
        pub next_epoch_data: EpochData,
    }

    pub fn protocol_state_view(state: &MinaStateProtocolStateValueStableV2) -> ProtocolStateView {
        let cs = &state.body.consensus_state;
        let sed = &cs.staking_epoch_data;
        let ned = &cs.staking_epoch_data;

        ProtocolStateView {
            snarked_ledger_hash: state.body.blockchain_state.registers.ledger.to_field(),
            timestamp: BlockTime(state.body.blockchain_state.timestamp.as_u64()),
            blockchain_length: Length(cs.blockchain_length.as_u32()),
            min_window_density: Length(cs.min_window_density.as_u32()),
            last_vrf_output: (),
            total_currency: Amount(cs.total_currency.as_u64()),
            global_slot_since_hard_fork: Slot(cs.curr_global_slot.slot_number.as_u32()), // TODO: Check if it's correct
            global_slot_since_genesis: Slot(cs.global_slot_since_genesis.as_u32()),
            staking_epoch_data: EpochData {
                ledger: EpochLedger {
                    hash: sed.ledger.hash.to_field(),
                    total_currency: Amount(sed.ledger.total_currency.as_u64()),
                },
                seed: sed.seed.to_field(),
                start_checkpoint: sed.start_checkpoint.to_field(),
                lock_checkpoint: sed.lock_checkpoint.to_field(),
                epoch_length: Length(sed.epoch_length.as_u32()),
            },
            next_epoch_data: EpochData {
                ledger: EpochLedger {
                    hash: ned.ledger.hash.to_field(),
                    total_currency: Amount(ned.ledger.total_currency.as_u64()),
                },
                seed: ned.seed.to_field(),
                start_checkpoint: ned.start_checkpoint.to_field(),
                lock_checkpoint: ned.lock_checkpoint.to_field(),
                epoch_length: Length(ned.epoch_length.as_u32()),
            },
        }
    }

    #[derive(Clone)]
    pub struct GlobalState<L: LedgerIntf + Clone> {
        pub ledger: L,
        pub fee_excess: Signed<Fee>,
        pub protocol_state: ProtocolStateView,
    }
}

pub mod local_state {
    use ark_ff::Zero;

    use crate::{
        hash_with_kimchi,
        scan_state::currency::{Index, Signed},
        Inputs,
    };

    use super::{zkapp_command::CallForest, *};

    #[derive(Clone)]
    pub struct StackFrame {
        pub caller: TokenId,
        pub caller_caller: TokenId,
        pub calls: CallForest<()>, // TODO
    }

    impl Default for StackFrame {
        fn default() -> Self {
            StackFrame {
                caller: TokenId::default(),
                caller_caller: TokenId::default(),
                calls: CallForest::new(),
            }
        }
    }

    impl StackFrame {
        pub fn empty() -> Self {
            Self {
                caller: TokenId::default(),
                caller_caller: TokenId::default(),
                calls: CallForest(Vec::new()),
            }
        }

        /// TODO: this needs to be tested
        ///
        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/stack_frame.ml#L90
        pub fn hash(&self) -> Fp {
            let mut inputs = Inputs::new();

            inputs.append_field(self.caller.0);
            inputs.append_field(self.caller_caller.0);

            let field = match self.calls.0.get(0) {
                None => Fp::zero(),
                Some(call) => call.stack_hash,
            };
            inputs.append_field(field);

            hash_with_kimchi("MinaAcctUpdStckFrm", &inputs.to_fields())
        }
    }

    #[derive(Clone)]
    pub struct CallStack(Vec<StackFrame>);

    impl CallStack {
        pub fn new() -> Self {
            CallStack(Vec::new())
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn iter(&self) -> impl Iterator<Item = &StackFrame> {
            self.0.iter().rev()
        }

        pub fn push(&self, stack_frame: &StackFrame) -> Self {
            let mut ret = self.0.clone();
            ret.push(stack_frame.clone());
            Self(ret)
        }

        pub fn pop(&self) -> Option<(StackFrame, CallStack)> {
            let mut ret = self.0.clone();
            if let Some(frame) = ret.pop() {
                Some((frame, Self(ret)))
            } else {
                None
            }
        }

        pub fn pop_exn(&self) -> (StackFrame, CallStack) {
            let mut ret = self.0.clone();
            if let Some(frame) = ret.pop() {
                (frame, Self(ret))
            } else {
                panic!()
            }
        }
    }

    /// NOTE: It looks like there are different instances of the polymorphic LocalEnv type
    /// One with concrete types for the stack frame, call stack, and ledger. Created from the Env
    /// And the other with their hashes. To differentiate them I renamed the first LocalStateEnv
    /// Maybe a better solution is to keep the LocalState name and put it under a different module
    #[derive(Clone)]
    pub struct LocalStateEnv<L: LedgerIntf + Clone> {
        pub stack_frame: StackFrame,
        pub call_stack: CallStack,
        pub transaction_commitment: ReceiptChainHash,
        pub full_transaction_commitment: ReceiptChainHash,
        pub token_id: TokenId,
        pub excess: Signed<Fee>,
        pub ledger: L,
        pub success: bool,
        pub account_update_index: Index,
        // TODO: optimize by reversing the insertion order
        pub failure_status_tbl: Vec<Vec<TransactionFailure>>,
    }

    impl<L: LedgerIntf + Clone> LocalStateEnv<L> {
        pub fn add_new_failure_status_bucket(&self) -> Self {
            let mut failure_status_tbl = self.failure_status_tbl.clone();
            failure_status_tbl.insert(0, Vec::new());
            Self {
                failure_status_tbl,
                ..self.clone()
            }
        }

        pub fn add_check(&self, failure: TransactionFailure, b: bool) -> Self {
            let failure_status_tbl = if let false = b {
                let mut failure_status_tbl = self.failure_status_tbl.clone();
                failure_status_tbl[0].insert(0, failure);
                failure_status_tbl
            } else {
                self.failure_status_tbl.clone()
            };

            Self {
                failure_status_tbl,
                success: self.success && b,
                ..self.clone()
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LocalState {
        pub stack_frame: Fp,
        pub call_stack: Fp,
        pub transaction_commitment: Fp,
        pub full_transaction_commitment: Fp,
        pub token_id: TokenId,
        pub excess: Signed<Amount>,
        pub supply_increase: Signed<Amount>,
        pub ledger: Fp,
        pub success: bool,
        pub account_update_index: Index,
        pub failure_status_tbl: Vec<Vec<TransactionFailure>>,
    }

    impl LocalState {
        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_state/local_state.ml#L63
        pub fn dummy() -> Self {
            Self {
                stack_frame: StackFrame::empty().hash(),
                call_stack: Fp::zero(),
                transaction_commitment: Fp::zero(),
                full_transaction_commitment: Fp::zero(),
                token_id: TokenId::default(),
                excess: Signed::<Amount>::zero(),
                supply_increase: Signed::<Amount>::zero(),
                ledger: Fp::zero(),
                success: true,
                account_update_index: Index(0),
                failure_status_tbl: Vec::new(),
            }
        }

        pub fn empty() -> Self {
            Self::dummy()
        }
    }
}

pub enum Eff<L: LedgerIntf + Clone> {
    CheckAccountPrecondition(AccountUpdate, Account, bool, LocalStateEnv<L>),
    CheckProtocolStatePrecondition(ZkAppPreconditions, GlobalState<L>),
    InitAccount(AccountUpdate, Account),
}

pub struct Env<L: LedgerIntf + Clone> {
    account_update: AccountUpdate,
    zkapp_command: ZkAppCommand,
    account: Account,
    ledger: L,
    amount: Amount,
    signed_amount: Signed<Amount>,
    bool: bool,
    token_id: TokenId,
    global_state: GlobalState<L>,
    local_state: LocalStateEnv<L>,
    protocol_state_precondition: ZkAppPreconditions,
    transaction_commitment: Fp,
    full_transaction_commitment: Fp,
    field: Fp,
    failure: Option<TransactionFailure>,
}

pub enum PerformResult<L: LedgerIntf + Clone> {
    Bool(bool),
    LocalState(LocalStateEnv<L>),
    Account(Account),
}

impl<L> Env<L>
where
    L: LedgerIntf + Clone,
{
    pub fn perform(eff: Eff<L>) -> PerformResult<L> {
        match eff {
            Eff::CheckProtocolStatePrecondition(pred, global_state) => {
                PerformResult::Bool(pred.check(global_state.protocol_state).is_ok())
            }
            Eff::CheckAccountPrecondition(account_update, account, new_account, local_state) => {
                let local_state = match account_update.body.preconditions.account {
                    AccountPreconditions::Accept => local_state,
                    AccountPreconditions::Nonce(n) => local_state.add_check(
                        TransactionFailure::AccountNoncePreconditionUnsatisfied,
                        account.nonce == n,
                    ),
                    AccountPreconditions::Full(precondition_account) => {
                        let mut _local_state = local_state;
                        let check = |failure, b| {
                            _local_state = _local_state.add_check(failure, b);
                        };
                        precondition_account.check(new_account, check, account);
                        _local_state
                    }
                };
                PerformResult::LocalState(local_state)
            }
            Eff::InitAccount(_account_update, a) => PerformResult::Account(a),
        }
    }
}

fn step_all<L>(
    constraint_constants: &ConstraintConstants,
    f: fn(
        Option<(LocalStateEnv<L>, Signed<Fee>)>,
        (GlobalState<L>, LocalStateEnv<L>),
    ) -> Option<(LocalStateEnv<L>, Signed<Fee>)>,
    h: fn(Eff<L>) -> PerformResult<L>,
    user_acc: Option<(LocalStateEnv<L>, Signed<Fee>)>,
    (g_state, l_state): (GlobalState<L>, LocalStateEnv<L>),
) -> Result<
    (
        Option<(LocalStateEnv<L>, Signed<Fee>)>,
        Vec<Vec<TransactionFailure>>,
    ),
    String,
>
where
    L: LedgerIntf + Clone,
{
    if l_state.stack_frame.calls.is_empty() {
        Ok((user_acc, l_state.failure_status_tbl))
    } else {
        let states = apply(
            constraint_constants,
            IsStart::No,
            Handler { perform: h },
            (g_state, l_state),
        );
        step_all(
            constraint_constants,
            f,
            h,
            f(user_acc, states.clone()),
            states,
        )
    }
}

fn apply_zkapp_command_unchecked<L>(
    constraint_constants: &ConstraintConstants,
    state_view: &ProtocolStateView,
    ledger: &mut L,
    c: &ZkAppCommand,
) -> Result<(ZkappCommandApplied, (LocalStateEnv<L>, Signed<Fee>)), String>
where
    L: LedgerIntf + Clone,
{
    let (account_update_applied, state_res) = apply_zkapp_command_unchecked_aux(
        constraint_constants,
        state_view,
        None,
        |_acc, (global_state, local_state)| Some((local_state, global_state.fee_excess)),
        None,
        ledger,
        c,
    )?;

    Ok((account_update_applied, state_res.unwrap()))
}

fn apply_zkapp_command_unchecked_aux<L>(
    constraint_constants: &ConstraintConstants,
    state_view: &ProtocolStateView,
    init: Option<(LocalStateEnv<L>, Signed<Fee>)>,
    f: fn(
        Option<(LocalStateEnv<L>, Signed<Fee>)>,
        (GlobalState<L>, LocalStateEnv<L>),
    ) -> Option<(LocalStateEnv<L>, Signed<Fee>)>,
    fee_excess: Option<Signed<Fee>>,
    ledger: &mut L,
    c: &ZkAppCommand,
) -> Result<(ZkappCommandApplied, Option<(LocalStateEnv<L>, Signed<Fee>)>), String>
where
    L: LedgerIntf + Clone,
{
    let fee_excess = fee_excess.unwrap_or_else(Signed::<Fee>::zero);
    let perform = |eff| Env::perform(eff);
    let accounts_accessed = c.accounts_accessed(TransactionStatus::Applied);
    let original_account_states = accounts_accessed.iter().map(|id| {
        (id, {
            let loc = ledger.location_of_account(id);
            let account = loc.as_ref().and_then(|loc| ledger.get(loc));
            loc.zip(account)
        })
    });

    let initial_state = (
        GlobalState {
            ledger: ledger.clone(),
            fee_excess,
            protocol_state: state_view.clone(),
        },
        LocalStateEnv {
            stack_frame: StackFrame::default(),
            call_stack: CallStack::new(),
            transaction_commitment: ReceiptChainHash(Fp::zero()),
            full_transaction_commitment: ReceiptChainHash(Fp::zero()),
            token_id: TokenId::default(),
            excess: Signed::<Fee>::zero(),
            ledger: ledger.clone(),
            success: true,
            account_update_index: Index::zero(),
            failure_status_tbl: Vec::new(),
        },
    );

    let user_acc = f(init, initial_state.clone());
    let start = {
        let zkapp_command = c.clone();
        apply(
            constraint_constants,
            IsStart::Yes(StartData {
                zkapp_command,
                memo_hash: c.memo.hash(),
            }),
            Handler { perform },
            initial_state,
        )
    };

    let accounts_accessed = c.accounts_accessed(TransactionStatus::Applied);

    let mut account_states_after_fee_payer = accounts_accessed.iter().map(|id| {
        let loc = ledger.location_of_account(id);
        let a = loc.as_ref().and_then(|loc| ledger.get(loc));

        match a {
            Some(a) => (id, Some((loc.unwrap(), a))),
            None => (id, None),
        }
    });

    let _original_account_states = original_account_states.clone();
    let accounts = || {
        _original_account_states.map(|(id, account)| (id.clone(), account.map(|(_loc, acc)| acc)))
    };

    match step_all(
        constraint_constants,
        f,
        perform,
        f(user_acc, start.clone()),
        start,
    ) {
        Err(e) => Err(e),
        Ok((s, failure_status_tbl)) => {
            let account_ids_originally_not_in_ledger =
                original_account_states.filter_map(|(acct_id, loc_and_acct)| {
                    if loc_and_acct.is_none() {
                        Some(acct_id)
                    } else {
                        None
                    }
                });
            let successfully_applied = failure_status_tbl.is_empty();
            let new_accounts = account_ids_originally_not_in_ledger
                .filter_map(|acct_id| {
                    let loc = ledger.location_of_account(acct_id);
                    let acc = loc.and_then(|loc| ledger.get(&loc));
                    match acc {
                        Some(acc) if acc.id() == *acct_id => Some(acct_id.clone()),
                        _ => None,
                    }
                })
                .collect::<Vec<AccountId>>();

            let valid_result = Ok((
                ZkappCommandApplied {
                    accounts: accounts().collect(),
                    command: WithStatus::<ZkAppCommand> {
                        data: c.clone(),
                        status: if successfully_applied {
                            TransactionStatus::Applied
                        } else {
                            TransactionStatus::Failed(failure_status_tbl)
                        },
                    },
                    new_accounts: new_accounts.clone(),
                },
                s,
            ));

            if successfully_applied {
                valid_result
            } else {
                let other_account_update_accounts_unchanged = account_states_after_fee_payer
                    .fold_while(true, |acc, (_, loc_opt)| match loc_opt {
                        Some((loc, a)) => match ledger.get(&loc) {
                            Some(a_) if !(a == a_) => Done(false),
                            _ => Continue(acc),
                        },
                        _ => Continue(acc),
                    })
                    .into_inner();
                if new_accounts.is_empty() && other_account_update_accounts_unchanged {
                    valid_result
                } else {
                    Err("Zkapp_command application failed but new accounts created or some of the other account_update updates applied".to_string())
                }
            }
        }
    }
}

pub fn apply_transaction<L>(
    constraint_constants: &ConstraintConstants,
    txn_state_view: &ProtocolStateView,
    ledger: &mut L,
    transaction: &Transaction,
) -> Result<TransactionApplied, String>
where
    L: LedgerIntf + Clone,
{
    use Transaction::*;
    use UserCommand::*;

    let previous_hash = ledger.merkle_root();
    let txn_global_slot = &txn_state_view.global_slot_since_genesis;

    match transaction {
        Command(SignedCommand(cmd)) => apply_user_command(
            constraint_constants,
            txn_state_view,
            txn_global_slot,
            ledger,
            cmd,
        )
        .map(|applied| Varying::Command(CommandApplied::SignedCommand(Box::new(applied)))),
        Command(ZkAppCommand(c)) => {
            apply_zkapp_command_unchecked(constraint_constants, txn_state_view, ledger, c).map(
                |(applied, _)| Varying::Command(CommandApplied::ZkappCommand(Box::new(applied))),
            )
        }
        FeeTransfer(fee_transfer) => {
            apply_fee_transfer(constraint_constants, txn_global_slot, ledger, fee_transfer)
                .map(Varying::FeeTransfer)
        }
        Coinbase(coinbase) => {
            apply_coinbase(constraint_constants, txn_global_slot, ledger, coinbase)
                .map(Varying::Coinbase)
        }
    }
    .map(|varying| TransactionApplied {
        previous_hash,
        varying,
    })
}

/// Structure of the failure status:
///  I. No fee transfer and coinbase transfer fails: [[failure]]
///  II. With fee transfer-
///   Both fee transfer and coinbase fails:
///     [[failure-of-fee-transfer]; [failure-of-coinbase]]
///   Fee transfer succeeds and coinbase fails:
///     [[];[failure-of-coinbase]]
///   Fee transfer fails and coinbase succeeds:
///     [[failure-of-fee-transfer];[]]
///
/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L2022
fn apply_coinbase<L>(
    constraint_constants: &ConstraintConstants,
    txn_global_slot: &Slot,
    ledger: &mut L,
    coinbase: &Coinbase,
) -> Result<transaction_applied::CoinbaseApplied, String>
where
    L: LedgerIntf,
{
    let Coinbase {
        receiver,
        amount: coinbase_amount,
        fee_transfer,
    } = &coinbase;

    let (
        receiver_reward,
        new_accounts1,
        transferee_update,
        transferee_timing_prev,
        failures1,
        burned_tokens1,
    ) = match fee_transfer {
        None => (*coinbase_amount, None, None, None, vec![], Amount::zero()),
        Some(
            ft @ CoinbaseFeeTransfer {
                receiver_pk: transferee,
                fee,
            },
        ) => {
            assert_ne!(transferee, receiver);

            let transferee_id = ft.receiver();
            let fee = Amount::of_fee(fee);

            let receiver_reward = coinbase_amount
                .checked_sub(&fee)
                .ok_or_else(|| "Coinbase fee transfer too large".to_string())?;

            let (transferee_account, action, can_receive) =
                has_permission_to_receive(ledger, &transferee_id);
            let new_accounts = get_new_accounts(action, transferee_id.clone());

            let timing = update_timing_when_no_deduction(txn_global_slot, &transferee_account)?;

            let balance = {
                let amount = sub_account_creation_fee(constraint_constants, action, fee)?;
                add_amount(transferee_account.balance, amount)?
            };

            if can_receive.0 {
                let (_, mut transferee_account, transferee_location) =
                    ledger.get_or_create(&transferee_id)?;

                transferee_account.balance = balance;
                transferee_account.timing = timing;

                let timing = transferee_account.timing.clone();

                (
                    receiver_reward,
                    new_accounts,
                    Some((transferee_location, transferee_account)),
                    Some(timing),
                    vec![],
                    Amount::zero(),
                )
            } else {
                (
                    receiver_reward,
                    None,
                    None,
                    None,
                    vec![TransactionFailure::UpdateNotPermittedBalance],
                    fee,
                )
            }
        }
    };

    let receiver_id = AccountId::new(receiver.clone(), TokenId::default());
    let (receiver_account, action2, can_receive) = has_permission_to_receive(ledger, &receiver_id);
    let new_accounts2 = get_new_accounts(action2, receiver_id.clone());

    // Note: Updating coinbase receiver timing only if there is no fee transfer.
    // This is so as to not add any extra constraints in transaction snark for checking
    // "receiver" timings. This is OK because timing rules will not be violated when
    // balance increases and will be checked whenever an amount is deducted from the
    // account (#5973)

    let coinbase_receiver_timing = match transferee_timing_prev {
        None => update_timing_when_no_deduction(txn_global_slot, &receiver_account)?,
        Some(_) => receiver_account.timing.clone(),
    };

    let receiver_balance = {
        let amount = sub_account_creation_fee(constraint_constants, action2, receiver_reward)?;
        add_amount(receiver_account.balance, amount)?
    };

    let (failures2, burned_tokens2) = if can_receive.0 {
        let (_action2, mut receiver_account, receiver_location) =
            ledger.get_or_create(&receiver_id)?;

        receiver_account.balance = receiver_balance;
        receiver_account.timing = coinbase_receiver_timing;

        ledger.set(&receiver_location, receiver_account);

        (vec![], Amount::zero())
    } else {
        (
            vec![TransactionFailure::UpdateNotPermittedBalance],
            receiver_reward,
        )
    };

    if let Some((addr, account)) = transferee_update {
        ledger.set(&addr, account);
    };

    let burned_tokens = burned_tokens1
        .checked_add(&burned_tokens2)
        .ok_or_else(|| "burned tokens overflow".to_string())?;

    let failures = vec![failures1, failures2];
    let status = if failures.iter().all(Vec::is_empty) {
        TransactionStatus::Applied
    } else {
        TransactionStatus::Failed(failures)
    };

    let new_accounts: Vec<_> = [new_accounts1, new_accounts2]
        .into_iter()
        .flatten()
        .collect();

    Ok(transaction_applied::CoinbaseApplied {
        coinbase: WithStatus {
            data: coinbase.clone(),
            status,
        },
        new_accounts,
        burned_tokens,
    })
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L1991
fn apply_fee_transfer<L>(
    constraint_constants: &ConstraintConstants,
    txn_global_slot: &Slot,
    ledger: &mut L,
    fee_transfer: &FeeTransfer,
) -> Result<transaction_applied::FeeTransferApplied, String>
where
    L: LedgerIntf,
{
    let (new_accounts, failures, burned_tokens) = process_fee_transfer(
        ledger,
        fee_transfer,
        |action, _, balance, fee| {
            let amount = {
                let amount = Amount::of_fee(fee);
                sub_account_creation_fee(constraint_constants, action, amount)?
            };
            add_amount(balance, amount)
        },
        |account| update_timing_when_no_deduction(txn_global_slot, account),
    )?;

    let status = if failures.iter().all(Vec::is_empty) {
        TransactionStatus::Applied
    } else {
        TransactionStatus::Failed(failures)
    };

    Ok(transaction_applied::FeeTransferApplied {
        fee_transfer: WithStatus {
            data: fee_transfer.clone(),
            status,
        },
        new_accounts,
        burned_tokens,
    })
}

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L607
fn sub_account_creation_fee(
    constraint_constants: &ConstraintConstants,
    action: AccountState,
    amount: Amount,
) -> Result<Amount, String> {
    let fee = &constraint_constants.account_creation_fee;

    match action {
        AccountState::Added => {
            if let Some(amount) = amount.checked_sub(&Amount::of_fee(fee)) {
                return Ok(amount);
            }
            Err(format!(
                "Error subtracting account creation fee {:?}; transaction amount {:?} insufficient",
                fee, amount
            ))
        }
        AccountState::Existed => Ok(amount),
    }
}

fn update_timing_when_no_deduction(
    txn_global_slot: &Slot,
    account: &Account,
) -> Result<Timing, String> {
    validate_timing(account, Amount::zero(), txn_global_slot)
}

// /// TODO: Move this to the ledger
// /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_ledger/ledger.ml#L311
// fn get_or_create<L>(
//     ledger: &mut L,
//     account_id: &AccountId,
// ) -> Result<(AccountState, Account, Address), String>
// where
//     L: LedgerIntf,
// {
//     let location = ledger
//         .get_or_create_account(account_id.clone(), Account::initialize(account_id))
//         .map_err(|e| format!("{:?}", e))?;

//     let action = match location {
//         GetOrCreated::Added(_) => AccountState::Added,
//         GetOrCreated::Existed(_) => AccountState::Existed,
//     };

//     let addr = location.addr();

//     let account = ledger
//         .get(addr.clone())
//         .expect("get_or_create: Account was not found in the ledger after creation");

//     Ok((action, account, addr))
// }

fn get_new_accounts<T>(action: AccountState, data: T) -> Option<T> {
    match action {
        AccountState::Added => Some(data),
        AccountState::Existed => None,
    }
}

/// Structure of the failure status:
///  I. Only one fee transfer in the transaction (`One) and it fails:
///     [[failure]]
///  II. Two fee transfers in the transaction (`Two)-
///   Both fee transfers fail:
///     [[failure-of-first-fee-transfer]; [failure-of-second-fee-transfer]]
///   First succeeds and second one fails:
///     [[];[failure-of-second-fee-transfer]]
///   First fails and second succeeds:
///     [[failure-of-first-fee-transfer];[]]
fn process_fee_transfer<L, FunBalance, FunTiming>(
    ledger: &mut L,
    fee_transfer: &FeeTransfer,
    modify_balance: FunBalance,
    modify_timing: FunTiming,
) -> Result<(Vec<AccountId>, Vec<Vec<TransactionFailure>>, Amount), String>
where
    L: LedgerIntf,
    FunTiming: Fn(&Account) -> Result<Timing, String>,
    FunBalance: Fn(AccountState, &AccountId, Balance, &Fee) -> Result<Balance, String>,
{
    if !fee_transfer.fee_tokens().all(TokenId::is_default) {
        return Err("Cannot pay fees in non-default tokens.".to_string());
    }

    match &**fee_transfer {
        OneOrTwo::One(fee_transfer) => {
            let account_id = fee_transfer.receiver();
            let (a, action, can_receive) = has_permission_to_receive(ledger, &account_id);

            let timing = modify_timing(&a)?;
            let balance = modify_balance(action, &account_id, a.balance, &fee_transfer.fee)?;

            if can_receive.0 {
                let (_, mut account, loc) = ledger.get_or_create(&account_id)?;
                let new_accounts = get_new_accounts(action, account_id.clone());

                account.balance = balance;
                account.timing = timing;

                ledger.set(&loc, account);

                let new_accounts: Vec<_> = new_accounts.into_iter().collect();
                Ok((new_accounts, vec![], Amount::zero()))
            } else {
                Ok((vec![], single_failure(), Amount::of_fee(&fee_transfer.fee)))
            }
        }
        OneOrTwo::Two((fee_transfer1, fee_transfer2)) => {
            let account_id1 = fee_transfer1.receiver();
            let (a1, action1, can_receive1) = has_permission_to_receive(ledger, &account_id1);

            let account_id2 = fee_transfer2.receiver();

            if account_id1 == account_id2 {
                let fee = fee_transfer1
                    .fee
                    .checked_add(&fee_transfer2.fee)
                    .ok_or_else(|| "Overflow".to_string())?;

                let timing = modify_timing(&a1)?;
                let balance = modify_balance(action1, &account_id1, a1.balance, &fee)?;

                if can_receive1.0 {
                    let (_, mut a1, l1) = ledger.get_or_create(&account_id1)?;
                    let new_accounts1 = get_new_accounts(action1, account_id1);

                    a1.balance = balance;
                    a1.timing = timing;

                    ledger.set(&l1, a1);

                    let new_accounts: Vec<_> = new_accounts1.into_iter().collect();
                    Ok((new_accounts, vec![vec![], vec![]], Amount::zero()))
                } else {
                    // failure for each fee transfer single

                    Ok((
                        vec![],
                        vec![
                            vec![TransactionFailure::UpdateNotPermittedBalance],
                            vec![TransactionFailure::UpdateNotPermittedBalance],
                        ],
                        Amount::of_fee(&fee),
                    ))
                }
            } else {
                let (a2, action2, can_receive2) = has_permission_to_receive(ledger, &account_id2);

                let balance1 =
                    modify_balance(action1, &account_id1, a1.balance, &fee_transfer1.fee)?;

                // Note: Not updating the timing field of a1 to avoid additional check
                // in transactions snark (check_timing for "receiver"). This is OK
                // because timing rules will not be violated when balance increases
                // and will be checked whenever an amount is deducted from the account. (#5973)*)

                let timing2 = modify_timing(&a2)?;
                let balance2 =
                    modify_balance(action2, &account_id2, a2.balance, &fee_transfer2.fee)?;

                let (new_accounts1, failures1, burned_tokens1) = if can_receive1.0 {
                    let (_, mut a1, l1) = ledger.get_or_create(&account_id1)?;
                    let new_accounts1 = get_new_accounts(action1, account_id1);

                    a1.balance = balance1;
                    ledger.set(&l1, a1);

                    (new_accounts1, vec![], Amount::zero())
                } else {
                    (
                        None,
                        vec![TransactionFailure::UpdateNotPermittedBalance],
                        Amount::of_fee(&fee_transfer1.fee),
                    )
                };

                let (new_accounts2, failures2, burned_tokens2) = if can_receive2.0 {
                    let (_, mut a2, l2) = ledger.get_or_create(&account_id2)?;
                    let new_accounts2 = get_new_accounts(action2, account_id2);

                    a2.balance = balance2;
                    a2.timing = timing2;

                    ledger.set(&l2, a2);

                    (new_accounts2, vec![], Amount::zero())
                } else {
                    (
                        None,
                        vec![TransactionFailure::UpdateNotPermittedBalance],
                        Amount::of_fee(&fee_transfer2.fee),
                    )
                };

                let burned_tokens = burned_tokens1
                    .checked_add(&burned_tokens2)
                    .ok_or_else(|| "burned tokens overflow".to_string())?;

                let new_accounts: Vec<_> = [new_accounts1, new_accounts2]
                    .into_iter()
                    .flatten()
                    .collect();
                let failures: Vec<_> = [failures1, failures2].into_iter().collect();

                Ok((new_accounts, failures, burned_tokens))
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum AccountState {
    Added,
    Existed,
}

#[derive(Debug)]
struct HasPermissionToReceive(bool);

/// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/transaction_logic/mina_transaction_logic.ml#L1852
fn has_permission_to_receive<L>(
    ledger: &mut L,
    receiver_account_id: &AccountId,
) -> (Account, AccountState, HasPermissionToReceive)
where
    L: LedgerIntf,
{
    use crate::PermissionTo::*;
    use AccountState::*;

    let init_account = Account::initialize(receiver_account_id);

    match ledger.location_of_account(receiver_account_id) {
        None => {
            // new account, check that default permissions allow receiving
            let perm = init_account.has_permission_to(Receive);
            (init_account, Added, HasPermissionToReceive(perm))
        }
        Some(location) => match ledger.get(&location) {
            None => panic!("Ledger location with no account"),
            Some(receiver_account) => {
                let perm = receiver_account.has_permission_to(Receive);
                (receiver_account, Existed, HasPermissionToReceive(perm))
            }
        },
    }
}

pub fn validate_time(valid_until: &Slot, current_global_slot: &Slot) -> Result<(), String> {
    if current_global_slot <= valid_until {
        return Ok(());
    }

    Err(format!(
        "Current global slot {:?} greater than transaction expiry slot {:?}",
        current_global_slot, valid_until
    ))
}

pub fn is_timed(a: &Account) -> bool {
    matches!(&a.timing, Timing::Timed { .. })
}

pub fn set_with_location<L>(
    ledger: &mut L,
    location: &ExistingOrNew<L::Location>,
    account: Account,
) -> Result<(), String>
where
    L: LedgerIntf,
{
    match location {
        ExistingOrNew::Existing(location) => {
            ledger.set(location, account);
            Ok(())
        }
        ExistingOrNew::New => ledger
            .create_new_account(account.id(), account)
            .map_err(|_| "set_with_location".to_string()),
    }
}

pub struct Updates<Location> {
    pub located_accounts: Vec<(ExistingOrNew<Location>, Account)>,
    pub applied_body: signed_command_applied::Body,
}

pub fn compute_updates<L>(
    constraint_constants: &ConstraintConstants,
    source: AccountId,
    receiver: AccountId,
    ledger: &mut L,
    current_global_slot: &Slot,
    user_command: &SignedCommand,
) -> Result<Updates<L::Location>, TransactionFailure>
where
    L: LedgerIntf,
{
    match &user_command.payload.body {
        signed_command::Body::StakeDelegation(_) => {
            let (source_location, mut source_account) = get_with_location(ledger, &source).unwrap();

            if !source_account.has_permission_to(PermissionTo::SetDelegate) {
                return Err(TransactionFailure::UpdateNotPermittedDelegate);
            }

            if let ExistingOrNew::New = source_location {
                return Err(TransactionFailure::SourceNotPresent);
            }

            let (receiver_location, _) = get_with_location(ledger, &receiver).unwrap();

            if let ExistingOrNew::New = receiver_location {
                return Err(TransactionFailure::ReceiverNotPresent);
            }

            let previous_delegate = source_account.delegate.clone();
            let timing = timing_error_to_user_command_status(validate_timing(
                &source_account,
                Amount::zero(),
                current_global_slot,
            ))?;

            source_account.delegate = Some(receiver.public_key.clone());
            source_account.timing = timing;

            Ok(Updates {
                located_accounts: vec![(source_location, source_account)],
                applied_body: signed_command_applied::Body::StakeDelegation { previous_delegate },
            })
        }
        signed_command::Body::Payment(payment) => {
            let (receiver_location, mut receiver_account) =
                get_with_location(ledger, &receiver).unwrap();

            if !receiver_account.has_permission_to(PermissionTo::Receive) {
                return Err(TransactionFailure::UpdateNotPermittedBalance);
            }

            let (source_location, source_account) = if source == receiver {
                let addr = match receiver_location.clone() {
                    ExistingOrNew::Existing(addr) => addr,
                    ExistingOrNew::New => return Err(TransactionFailure::SourceNotPresent),
                };

                let timing = timing_error_to_user_command_status(validate_timing(
                    &receiver_account,
                    payment.amount,
                    current_global_slot,
                ))?;

                receiver_account.timing = timing;

                (ExistingOrNew::Existing(addr), receiver_account.clone())
            } else {
                let (location, mut account) = get_with_location(ledger, &source).unwrap();

                if let ExistingOrNew::New = location {
                    return Err(TransactionFailure::SourceNotPresent);
                }

                let timing = timing_error_to_user_command_status(validate_timing(
                    &account,
                    payment.amount,
                    current_global_slot,
                ))?;

                let balance = match account.balance.sub_amount(payment.amount) {
                    Some(balance) => balance,
                    None => return Err(TransactionFailure::SourceInsufficientBalance),
                };

                account.timing = timing;
                account.balance = balance;

                (location, account)
            };

            if !source_account.has_permission_to(PermissionTo::Send) {
                return Err(TransactionFailure::UpdateNotPermittedBalance);
            }

            let receiver_amount = match &receiver_location {
                ExistingOrNew::Existing(_) => payment.amount,
                ExistingOrNew::New => {
                    match payment
                        .amount
                        .checked_sub(&Amount::of_fee(&constraint_constants.account_creation_fee))
                    {
                        Some(amount) => amount,
                        None => return Err(TransactionFailure::AmountInsufficientToCreateAccount),
                    }
                }
            };

            let balance = match receiver_account.balance.add_amount(receiver_amount) {
                Some(balance) => balance,
                None => return Err(TransactionFailure::Overflow),
            };

            let new_accounts = match receiver_location {
                ExistingOrNew::New => vec![receiver.clone()],
                ExistingOrNew::Existing(_) => vec![],
            };

            receiver_account.balance = balance;

            Ok(Updates {
                located_accounts: vec![
                    (receiver_location, receiver_account),
                    (source_location, source_account),
                ],
                applied_body: signed_command_applied::Body::Payments { new_accounts },
            })
        }
    }
}

pub fn apply_user_command_unchecked<L>(
    constraint_constants: &ConstraintConstants,
    _txn_state_view: &ProtocolStateView,
    txn_global_slot: &Slot,
    ledger: &mut L,
    user_command: &SignedCommand,
) -> Result<SignedCommandApplied, String>
where
    L: LedgerIntf,
{
    let SignedCommand {
        payload: _,
        signer: signer_pk,
        signature: _,
    } = &user_command;
    let current_global_slot = txn_global_slot;

    let valid_until = user_command.valid_until();
    validate_time(&valid_until, current_global_slot)?;

    // Fee-payer information
    let _fee_payer = user_command.fee_payer();
    let (fee_payer_location, fee_payer_account) =
        pay_fee(user_command, signer_pk, ledger, current_global_slot)?;

    if !fee_payer_account.has_permission_to(PermissionTo::Send) {
        return Err(TransactionFailure::UpdateNotPermittedBalance.to_string());
    }

    set_with_location(ledger, &fee_payer_location, fee_payer_account)?;

    let source = user_command.source();
    let receiver = user_command.receiver();

    match compute_updates(
        constraint_constants,
        source,
        receiver,
        ledger,
        current_global_slot,
        user_command,
    ) {
        Ok(Updates {
            located_accounts,
            applied_body,
        }) => {
            for (location, account) in located_accounts {
                set_with_location(ledger, &location, account)?;
            }

            Ok(SignedCommandApplied {
                common: signed_command_applied::Common {
                    user_command: WithStatus::<SignedCommand> {
                        data: user_command.clone(),
                        status: TransactionStatus::Applied,
                    },
                },
                body: applied_body,
            })
        }
        Err(failure) => Ok(SignedCommandApplied {
            common: signed_command_applied::Common {
                user_command: WithStatus::<SignedCommand> {
                    data: user_command.clone(),
                    status: TransactionStatus::Failed(vec![vec![failure]]),
                },
            },
            body: signed_command_applied::Body::Failed,
        }),
    }
}

pub fn apply_user_command<L>(
    constraint_constants: &ConstraintConstants,
    txn_state_view: &ProtocolStateView,
    txn_global_slot: &Slot,
    ledger: &mut L,
    user_command: &SignedCommand,
) -> Result<SignedCommandApplied, String>
where
    L: LedgerIntf,
{
    apply_user_command_unchecked(
        constraint_constants,
        txn_state_view,
        txn_global_slot,
        ledger,
        user_command,
    )
}

pub fn pay_fee<L, Loc>(
    user_command: &SignedCommand,
    signer_pk: &CompressedPubKey,
    ledger: &mut L,
    current_global_slot: &Slot,
) -> Result<(ExistingOrNew<Loc>, Account), String>
where
    L: LedgerIntf<Location = Loc>,
{
    let nonce = user_command.nonce();
    let fee_payer = user_command.fee_payer();
    let fee_token = user_command.fee_token();

    if &fee_payer.public_key != signer_pk {
        return Err("Cannot pay fees from a public key that did not sign the transaction".into());
    }

    if fee_token != TokenId::default() {
        return Err("Cannot create transactions with fee_token different from the default".into());
    }

    pay_fee_impl(
        &user_command.payload,
        nonce,
        fee_payer,
        user_command.fee(),
        ledger,
        current_global_slot,
    )
}

fn pay_fee_impl<L>(
    command: &SignedCommandPayload,
    nonce: Nonce,
    fee_payer: AccountId,
    fee: Fee,
    ledger: &mut L,
    current_global_slot: &Slot,
) -> Result<(ExistingOrNew<L::Location>, Account), String>
where
    L: LedgerIntf,
{
    // Fee-payer information
    let (location, mut account) = get_with_location(ledger, &fee_payer)?;

    if let ExistingOrNew::New = location {
        return Err("The fee-payer account does not exist".to_string());
    };

    let fee = Amount::of_fee(&fee);
    let balance = sub_amount(account.balance, fee)?;

    validate_nonces(nonce, account.nonce)?;
    let timing = validate_timing(&account, fee, current_global_slot)?;

    account.balance = balance;
    account.nonce = account.nonce.incr(); // TODO: Not sure if OCaml wraps
    account.receipt_chain_hash = cons_signed_command_payload(command, account.receipt_chain_hash);
    account.timing = timing;

    Ok((location, account))

    // in
    // ( location
    // , { account with
    //     balance
    //   ; nonce = Account.Nonce.succ account.nonce
    //   ; receipt_chain_hash =
    //       Receipt.Chain_hash.cons_signed_command_payload command
    //         account.receipt_chain_hash
    //   ; timing
    //   } )
}

pub mod transaction_union_payload {
    use mina_hasher::ROInput as LegacyInput;
    use mina_signer::NetworkId;
    use static_assertions::const_assert_eq;

    use crate::scan_state::transaction_logic::signed_command::{
        PaymentPayload, StakeDelegationPayload,
    };

    use super::*;

    #[derive(Clone)]
    struct Common {
        fee: Fee,
        fee_token: TokenId,
        fee_payer_pk: CompressedPubKey,
        nonce: Nonce,
        valid_until: Slot,
        memo: Memo,
    }

    #[derive(Clone, Debug)]
    enum Tag {
        Payment = 0,
        StakeDelegation = 1,
        CreateAccount = 2,
        MintTokens = 3,
        FeeTransfer = 4,
        Coinbase = 5,
    }

    #[derive(Clone)]
    struct Body {
        tag: Tag,
        source_pk: CompressedPubKey,
        receiver_pk: CompressedPubKey,
        token_id: TokenId,
        amount: Amount,
        token_locked: bool,
    }

    #[derive(Clone)]
    pub struct TransactionUnionPayload {
        common: Common,
        body: Body,
    }

    impl mina_hasher::Hashable for TransactionUnionPayload {
        type D = mina_signer::NetworkId;

        fn to_roinput(&self) -> LegacyInput {
            self.to_input_legacy()
        }

        fn domain_string(domain_param: Self::D) -> Option<String> {
            const S: &[&str] = &["MinaSignatureMainnet", "CodaSignature"];
            assert_eq!(NetworkId::TESTNET as usize, 0);
            assert_eq!(NetworkId::MAINNET as usize, 1);

            Some(
                S.get(domain_param as usize)
                    .map(ToString::to_string)
                    .unwrap(),
            )
        }
    }

    impl TransactionUnionPayload {
        pub fn of_user_command_payload(payload: &SignedCommandPayload) -> Self {
            use signed_command::Body::{Payment, StakeDelegation};

            Self {
                common: Common {
                    fee: payload.common.fee,
                    fee_token: TokenId::default(),
                    fee_payer_pk: payload.common.fee_payer_pk.clone(),
                    nonce: payload.common.nonce,
                    valid_until: payload.common.valid_until,
                    memo: payload.common.memo.clone(),
                },
                body: match &payload.body {
                    Payment(PaymentPayload {
                        source_pk,
                        receiver_pk,
                        amount,
                    }) => Body {
                        tag: Tag::Payment,
                        source_pk: source_pk.clone(),
                        receiver_pk: receiver_pk.clone(),
                        token_id: TokenId::default(),
                        amount: *amount,
                        token_locked: false,
                    },
                    StakeDelegation(StakeDelegationPayload::SetDelegate {
                        delegator,
                        new_delegate,
                    }) => Body {
                        tag: Tag::StakeDelegation,
                        source_pk: delegator.clone(),
                        receiver_pk: new_delegate.clone(),
                        token_id: TokenId::default(),
                        amount: Amount::zero(),
                        token_locked: false,
                    },
                },
            }
        }

        /// https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/transaction_union_payload.ml#L309
        pub fn to_input_legacy(&self) -> LegacyInput {
            let mut roi = LegacyInput::new();

            // Self.common
            {
                roi = roi.append_u64(self.common.fee.0);

                // TokenId.default
                // https://github.com/MinaProtocol/mina/blob/2ee6e004ba8c6a0541056076aab22ea162f7eb3a/src/lib/mina_base/signed_command_payload.ml#L19
                roi = roi.append_bool(true);
                for _ in 0..63 {
                    roi = roi.append_bool(false);
                }

                // fee_payer_pk
                roi = roi.append_field(self.common.fee_payer_pk.x);
                roi = roi.append_bool(self.common.fee_payer_pk.is_odd);

                // nonce
                roi = roi.append_u32(self.common.nonce.0);

                // valid_until
                roi = roi.append_u32(self.common.valid_until.0);

                // memo
                roi = roi.append_bytes(&self.common.memo.0);
            }

            // Self.body
            {
                // tag
                let tag = self.body.tag.clone() as u8;
                for bit in [4, 2, 1] {
                    roi = roi.append_bool(tag & bit != 0);
                }

                // source_pk
                roi = roi.append_field(self.body.source_pk.x);
                roi = roi.append_bool(self.body.source_pk.is_odd);

                // receiver_pk
                roi = roi.append_field(self.body.receiver_pk.x);
                roi = roi.append_bool(self.body.receiver_pk.is_odd);

                // default token_id
                roi = roi.append_u64(1);

                // amount
                roi = roi.append_u64(self.body.amount.0);

                // token_locked
                roi = roi.append_bool(self.body.token_locked);
            }

            roi
        }
    }
}

/// Returns the new `receipt_chain_hash`
pub fn cons_signed_command_payload(
    command_payload: &SignedCommandPayload,
    last_receipt_chain_hash: ReceiptChainHash,
) -> ReceiptChainHash {
    // Note: Not sure why the use the legacy way of hashing here

    use mina_hasher::ROInput as LegacyInput;

    let union = TransactionUnionPayload::of_user_command_payload(command_payload);

    let inputs = union.to_input_legacy();
    let inputs = inputs.append_field(last_receipt_chain_hash.0);

    use mina_hasher::{create_legacy, Hashable, Hasher, ROInput};

    #[derive(Clone)]
    struct MyInput(LegacyInput);

    impl Hashable for MyInput {
        type D = ();

        fn to_roinput(&self) -> ROInput {
            self.0.clone()
        }

        fn domain_string(_: Self::D) -> Option<String> {
            Some("MinaReceiptUC".to_string())
        }
    }

    let mut hasher = create_legacy::<MyInput>(());
    hasher.update(&MyInput(inputs));
    ReceiptChainHash(hasher.digest())
}

fn validate_nonces(txn_nonce: Nonce, account_nonce: Nonce) -> Result<(), String> {
    if account_nonce == txn_nonce {
        return Ok(());
    }

    Err(format!(
        "Nonce in account {:?} different from nonce in transaction {:?}",
        account_nonce, txn_nonce,
    ))
}

pub fn validate_timing(
    account: &Account,
    txn_amount: Amount,
    txn_global_slot: &Slot,
) -> Result<Timing, String> {
    let (timing, _) = validate_timing_with_min_balance(account, txn_amount, txn_global_slot)?;

    Ok(timing)
}

pub fn account_check_timing(
    txn_global_slot: &Slot,
    account: Account,
) -> (TimingValidation, Timing) {
    let (invalid_timing, _timing, _) =
        validate_timing_with_min_balance_impl(&account, Amount::from_u64(0), txn_global_slot);
    // TODO: In OCaml the returned Timing is actually converted to None/Some(fields of Timing structure)
    (invalid_timing, account.timing)
}

fn validate_timing_with_min_balance(
    account: &Account,
    txn_amount: Amount,
    txn_global_slot: &Slot,
) -> Result<(Timing, MinBalance), String> {
    use TimingValidation::*;

    let (possibly_error, timing, min_balance) =
        validate_timing_with_min_balance_impl(account, txn_amount, txn_global_slot);

    match possibly_error {
        InsufficientBalance(true) => Err(format!(
            "For timed account, the requested transaction for amount {:?} \
             at global slot {:?}, the balance {:?} \
             is insufficient",
            txn_amount, txn_global_slot, account.balance
        )),
        InvalidTiming(true) => Err(format!(
            "For timed account, the requested transaction for amount {:?} \
             at global slot {:?}, applying the transaction would put the \
             balance below the calculated minimum balance of {:?}",
            txn_amount, txn_global_slot, min_balance.0
        )),
        InsufficientBalance(false) => {
            panic!("Broken invariant in validate_timing_with_min_balance'")
        }
        InvalidTiming(false) => Ok((timing, min_balance)),
    }
}

pub fn timing_error_to_user_command_status(
    timing_result: Result<Timing, String>,
) -> Result<Timing, TransactionFailure> {
    match timing_result {
        Ok(timing) => Ok(timing),
        Err(err_str) => {
            /*
                HACK: we are matching over the full error string instead
                of including an extra tag string to the Err variant
            */
            if err_str.contains("minimum balance") {
                return Err(TransactionFailure::SourceMinimumBalanceViolation);
            }

            if err_str.contains("is insufficient") {
                return Err(TransactionFailure::SourceInsufficientBalance);
            }

            panic!("Unexpected timed account validation error")
        }
    }
}

pub enum TimingValidation {
    InsufficientBalance(bool),
    InvalidTiming(bool),
}

#[derive(Debug)]
struct MinBalance(Balance);

fn validate_timing_with_min_balance_impl(
    account: &Account,
    txn_amount: Amount,
    txn_global_slot: &Slot,
) -> (TimingValidation, Timing, MinBalance) {
    use crate::Timing::*;
    use TimingValidation::*;

    match &account.timing {
        Untimed => {
            // no time restrictions
            match account.balance.sub_amount(txn_amount) {
                None => (
                    InsufficientBalance(true),
                    Untimed,
                    MinBalance(Balance::zero()),
                ),
                Some(_) => (InvalidTiming(false), Untimed, MinBalance(Balance::zero())),
            }
        }
        Timed {
            initial_minimum_balance,
            cliff_time,
            cliff_amount,
            vesting_period,
            vesting_increment,
        } => {
            let account_balance = account.balance;
            let initial_minimum_balance = initial_minimum_balance;

            let (invalid_balance, invalid_timing, curr_min_balance) =
                match account_balance.sub_amount(txn_amount) {
                    None => {
                        // NB: The [initial_minimum_balance] here is the incorrect value,
                        // but:
                        // * we don't use it anywhere in this error case; and
                        // * we don't want to waste time computing it if it will be unused.
                        (true, false, *initial_minimum_balance)
                    }
                    Some(proposed_new_balance) => {
                        let cliff_time = cliff_time;
                        let cliff_amount = cliff_amount;
                        let vesting_period = vesting_period;
                        let vesting_increment = vesting_increment;

                        let curr_min_balance = account_min_balance_at_slot(
                            *txn_global_slot,
                            *cliff_time,
                            *cliff_amount,
                            *vesting_period,
                            *vesting_increment,
                            *initial_minimum_balance,
                        );

                        if proposed_new_balance < curr_min_balance {
                            (false, true, curr_min_balance)
                        } else {
                            (false, false, curr_min_balance)
                        }
                    }
                };

            // once the calculated minimum balance becomes zero, the account becomes untimed
            let possibly_error = if invalid_balance {
                InsufficientBalance(invalid_balance)
            } else {
                InvalidTiming(invalid_timing)
            };

            if curr_min_balance > Balance::zero() {
                (
                    possibly_error,
                    account.timing.clone(),
                    MinBalance(curr_min_balance),
                )
            } else {
                (possibly_error, Untimed, MinBalance(Balance::zero()))
            }
        }
    }
}

// TODO: This should be in `account.rs`
pub fn account_min_balance_at_slot(
    global_slot: Slot,
    cliff_time: Slot,
    cliff_amount: Amount,
    vesting_period: Slot,
    vesting_increment: Amount,
    initial_minimum_balance: Balance,
) -> Balance {
    if global_slot < cliff_time {
        initial_minimum_balance
    } else if vesting_period.is_zero() {
        // If vesting period is zero then everything vests immediately at the cliff
        Balance::zero()
    } else {
        match initial_minimum_balance.sub_amount(cliff_amount) {
            None => Balance::zero(),
            Some(min_balance_past_cliff) => {
                // take advantage of fact that global slots are uint32's

                let num_periods =
                    (global_slot.as_u32() - cliff_time.as_u32()) / vesting_period.as_u32();
                let num_periods: u64 = num_periods.try_into().unwrap();

                let vesting_decrement = {
                    let vesting_increment = vesting_increment.as_u64();

                    if u64::MAX
                        .checked_div(num_periods)
                        .map(|res| matches!(res.cmp(&vesting_increment), std::cmp::Ordering::Less))
                        .unwrap_or(false)
                    {
                        // The vesting decrement will overflow, use [max_int] instead.
                        Amount::from_u64(u64::MAX)
                    } else {
                        Amount::from_u64(num_periods.checked_mul(vesting_increment).unwrap())
                    }
                };

                match min_balance_past_cliff.sub_amount(vesting_decrement) {
                    None => Balance::zero(),
                    Some(amount) => amount,
                }
            }
        }
    }
}

fn sub_amount(balance: Balance, amount: Amount) -> Result<Balance, String> {
    balance
        .sub_amount(amount)
        .ok_or_else(|| "insufficient funds".to_string())
}

fn add_amount(balance: Balance, amount: Amount) -> Result<Balance, String> {
    balance
        .add_amount(amount)
        .ok_or_else(|| "overflow".to_string())
}

#[derive(Clone, Debug)]
pub enum ExistingOrNew<Loc> {
    Existing(Loc),
    New,
}

fn get_with_location<L>(
    ledger: &mut L,
    account_id: &AccountId,
) -> Result<(ExistingOrNew<L::Location>, Account), String>
where
    L: LedgerIntf,
{
    match ledger.location_of_account(account_id) {
        Some(location) => match ledger.get(&location) {
            Some(account) => Ok((ExistingOrNew::Existing(location), account)),
            None => panic!("Ledger location with no account"),
        },
        None => Ok((
            ExistingOrNew::New,
            Account::create_with(account_id.clone(), Balance::zero()),
        )),
    }
}

pub fn get_account<L>(
    ledger: &mut L,
    account_id: AccountId,
) -> (Account, ExistingOrNew<L::Location>)
where
    L: LedgerIntf,
{
    let (loc, account) = get_with_location(ledger, &account_id).unwrap();
    (account, loc)
}

pub fn set_account<'a, L>(
    l: &'a mut L,
    (a, loc): (Account, &ExistingOrNew<L::Location>),
) -> &'a mut L
where
    L: LedgerIntf,
{
    set_with_location(l, loc, a).unwrap();
    l
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use o1_utils::FieldHelpers;

    use super::{
        signed_command::{Body, Common, PaymentPayload},
        *,
    };

    fn pub_key(address: &str) -> CompressedPubKey {
        mina_signer::PubKey::from_address(address)
            .unwrap()
            .into_compressed()
    }

    /// Test using same values as here:
    /// https://github.com/MinaProtocol/mina/blob/3a78f0e0c1343d14e2729c8b00205baa2ec70c93/src/lib/mina_base/receipt.ml#L136
    #[test]
    fn test_cons_receipt_hash_ocaml() {
        let from = pub_key("B62qr71UxuyKpkSKYceCPsjw14nuaeLwWKZdMqaBMPber5AAF6nkowS");
        let to = pub_key("B62qnvGVnU7FXdy8GdkxL7yciZ8KattyCdq5J6mzo5NCxjgQPjL7BTH");

        let common = Common {
            fee: Fee::from_u64(9758327274353182341),
            fee_payer_pk: from.clone(),
            nonce: Nonce::from_u32(1609569868),
            valid_until: Slot::from_u32(2127252111),
            memo: Memo([
                1, 32, 101, 26, 225, 104, 115, 118, 55, 102, 76, 118, 108, 78, 114, 50, 0, 115,
                110, 108, 53, 75, 109, 112, 50, 110, 88, 97, 76, 66, 76, 81, 235, 79,
            ]),
        };

        let body = Body::Payment(PaymentPayload {
            source_pk: from,
            receiver_pk: to,
            amount: Amount::from_u64(1155659205107036493),
        });

        let tx = SignedCommandPayload { common, body };

        let prev = "4918218371695029984164006552208340844155171097348169027410983585063546229555";
        let prev_receipt_chain_hash = ReceiptChainHash(Fp::from_str(prev).unwrap());

        let next = "11119245469205697592341599081188990695704663506019727849135180468159777463297";
        let next_receipt_chain_hash = ReceiptChainHash(Fp::from_str(next).unwrap());

        let result = cons_signed_command_payload(&tx, prev_receipt_chain_hash);
        assert_eq!(result, next_receipt_chain_hash);
    }

    #[test]
    fn test_receipt_hash_update() {
        let from = pub_key("B62qmnY6m4c6bdgSPnQGZriSaj9vuSjsfh6qkveGTsFX3yGA5ywRaja");
        let to = pub_key("B62qjVQLxt9nYMWGn45mkgwYfcz8e8jvjNCBo11VKJb7vxDNwv5QLPS");

        let common = Common {
            fee: Fee::from_u64(14500000),
            fee_payer_pk: from.clone(),
            nonce: Nonce::from_u32(15),
            valid_until: Slot::from_u32(-1i32 as u32),
            memo: Memo([
                1, 7, 84, 104, 101, 32, 49, 48, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]),
        };

        let body = Body::Payment(PaymentPayload {
            source_pk: from,
            receiver_pk: to,
            amount: Amount::from_u64(2354000000),
        });

        let tx = SignedCommandPayload { common, body };

        let mut prev =
            hex::decode("09ac04c9965b885acfc9c54141dbecfc63b2394a4532ea2c598d086b894bfb14")
                .unwrap();
        prev.reverse();
        let prev_receipt_chain_hash = ReceiptChainHash(Fp::from_bytes(&prev).unwrap());

        let mut next =
            hex::decode("0735169b96af4385c7345c94d4d65f83823309e95f72752d3f9d84f4282a53ac")
                .unwrap();
        next.reverse();
        let next_receipt_chain_hash = ReceiptChainHash(Fp::from_bytes(&next).unwrap());

        let result = cons_signed_command_payload(&tx, prev_receipt_chain_hash);
        assert_eq!(result, next_receipt_chain_hash);
    }
}
