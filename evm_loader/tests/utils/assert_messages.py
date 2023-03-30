class InstructionAsserts:
    LOCKED_ACC = "trying to execute transaction on rw locked account"
    INVALID_CHAIN_ID = "Invalid Chain ID"
    INVALID_NONCE = "Invalid Nonce"
    TRX_ALREADY_FINALIZED = "is finalized"
    INSUFFICIENT_FUNDS = "Insufficient balance"
    OUT_OF_GAS = "Out of Gas"
    ADDRESS_MUST_BE_PRESENT = r"address .* must be present in the transaction"
    INVALID_TREASURE_ACC = "invalid treasure account"
    ACC_NOT_FOUND = "AccountNotFound"
    NOT_AUTHORIZED_OPERATOR = "Operator is not authorized"
    NOT_SYSTEM_PROGRAM = "Account {} - is not system program"
    NOT_NEON_PROGRAM = "Account {} - is not Neon program"
    NOT_PROGRAM_OWNED = "Account {} - invalid owner"
    INVALID_HOLDER_OWNER = "Holder Account - invalid owner"
    INVALID_OPERATOR_KEY = "operator.key != storage.operator"
    HOLDER_OVERFLOW = "Checked Integer Math Overflow"
