import base64
import json
import os
import subprocess
import time
from enum import Enum
from hashlib import sha256
from typing import NamedTuple

import base58
import rlp
from base58 import b58encode
from construct import Bytes, Int8ul, Int64ul, Struct as cStruct
from eth_keys import keys as eth_keys
from sha3 import keccak_256
from solana._layouts.system_instructions import SYSTEM_INSTRUCTIONS_LAYOUT, InstructionType as SystemInstructionType
from solana.account import Account
from solana.publickey import PublicKey
from solana.rpc import types
from solana.rpc.api import Client
from solana.rpc.commitment import Confirmed
from solana.rpc.types import TxOpts
from solana.transaction import AccountMeta, TransactionInstruction, Transaction

from eth_tx_utils import make_keccak_instruction_data, make_instruction_data_from_tx
from spl.token.constants import TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, ACCOUNT_LEN
from spl.token.instructions import get_associated_token_address
import base58

CREATE_ACCOUNT_LAYOUT = cStruct(
    "lamports" / Int64ul,
    "space" / Int64ul,
    "ether" / Bytes(20),
    "nonce" / Int8ul
)

system = "11111111111111111111111111111111"
tokenkeg = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
sysvarclock = "SysvarC1ock11111111111111111111111111111111"
sysinstruct = "Sysvar1nstructions1111111111111111111111111"
keccakprog = "KeccakSecp256k11111111111111111111111111111"
rentid = "SysvarRent111111111111111111111111111111111"
incinerator = "1nc1nerator11111111111111111111111111111111"

collateral_pool_base = "4sW3SZDJB7qXUyCYKA7pFL8eCTfm3REr8oSiKkww7MaT"

solana_url = os.environ.get("SOLANA_URL", "http://localhost:8899")
# EVM_LOADER = os.environ.get("EVM_LOADER")
# ETH_TOKEN_MINT_ID: PublicKey = PublicKey(os.environ.get("ETH_TOKEN_MINT"))
EVM_LOADER="DZs4oh51XqbgneUKcHgLxES4f1LfME5oTndzcU8qxHvi"
ETH_TOKEN_MINT_ID : PublicKey = PublicKey("HPsV9Deocecw3GeZv1FkAPNCBRfuVyfw9MMwjwRe1xaU")
EVM_LOADER_SO = os.environ.get("EVM_LOADER_SO", 'target/bpfel-unknown-unknown/release/evm_loader.so')
client = Client(solana_url)
path_to_solana = 'solana'

class SplToken:
    def __init__(self, url):
        self.url = url

    def call(self, arguments):
        cmd = 'spl-token --url {} {}'.format(self.url, arguments)
        print('cmd:', cmd)
        try:
            return subprocess.check_output(cmd, shell=True, universal_newlines=True)
        except subprocess.CalledProcessError as err:
            import sys
            print("ERR: spl-token error {}".format(err))
            raise

    def transfer(self, mint, amount, recipient):
        self.call("transfer {} {} {}".format(mint, amount, recipient))

    def balance(self, acc):
        from decimal import Decimal
        res = self.call("balance --address {}".format(acc))
        return Decimal(res.rstrip())

    def mint(self, mint_id, recipient, amount, owner=None):
        if owner is None:
            self.call("mint {} {} {}".format(mint_id, amount, recipient))
        else:
            self.call("mint {} {} {} --owner {}".format(mint_id, amount, recipient, owner))
        print("minting {} tokens for {}".format(amount, recipient))

    def create_token(self, owner=None):
        if owner is None:
            res = self.call("create-token")
        else:
            res = self.call("create-token --owner {}".format(owner))
        if not res.startswith("Creating token "):
            raise Exception("create token error")
        else:
            return res.split()[2]

    def create_token_account(self, token, owner=None):
        if owner is None:
            res = self.call("create-account {}".format(token))
        else:
            res = self.call("create-account {} --owner {}".format(token, owner))
        if not res.startswith("Creating account "):
            raise Exception("create account error %s" % res)
        else:
            return res.split()[2]


class EthereumTransaction:
    """Encapsulate the all data of an ethereum transaction that should be executed."""

    def __init__(self, ether_caller, contract_account, contract_code_account, trx_data, account_metas=None, steps=500):
        self.ether_caller = ether_caller
        self.contract_account = contract_account
        self.contract_code_account = contract_code_account
        self.trx_data = trx_data
        self.trx_account_metas = account_metas
        self.iterative_steps = steps
        self._solana_ether_caller = None  # is created in NeonEvmClient.__create_instruction_data_from_tx
        self._storage = None  # is created in NeonEvmClient.__send_neon_transaction
        print('trx_data:', self.trx_data.hex())
        if self.trx_account_metas is not None:
            print('trx_account_metas:', *self.trx_account_metas, sep='\n')


class ExecuteMode(Enum):
    SINGLE = 0
    ITERATIVE = 1


class NeonEvmClient:
    """Encapsulate the interaction logic with evm_loader to execute an ethereum transaction."""

    def __init__(self, solana_wallet, evm_loader):
        self.mode = ExecuteMode.SINGLE
        self.solana_wallet = solana_wallet
        self.evm_loader = evm_loader
        self.token = SplToken(solana_url)

        collateral_pool_index = 2
        self.collateral_pool_address = create_collateral_pool_address(collateral_pool_index)
        self.collateral_pool_index_buf = collateral_pool_index.to_bytes(4, 'little')

    def set_execute_mode(self, new_mode):
        self.mode = ExecuteMode(new_mode)

    def send_ethereum_trx(self, ethereum_transaction) -> types.RPCResponse:
        assert (isinstance(ethereum_transaction, EthereumTransaction))
        if self.mode is ExecuteMode.SINGLE:
            return self.send_ethereum_trx_single(ethereum_transaction)
        if self.mode is ExecuteMode.ITERATIVE:
            return self.send_ethereum_trx_iterative(ethereum_transaction)

    def send_ethereum_trx_iterative(self, ethereum_transaction) -> types.RPCResponse:
        assert (isinstance(ethereum_transaction, EthereumTransaction))
        (from_address, sign, msg) = self.__create_instruction_data_from_tx(ethereum_transaction)
        if ethereum_transaction._storage is None:
            ethereum_transaction._storage = self.__create_storage_account(sign[:8].hex())

        data = from_address + sign + msg
        keccak_data = make_keccak_instruction_data(1, len(msg), 13)
        
        solana_trx = Transaction().add(
                self.__sol_instr_keccak(keccak_data) 
            ).add( 
                self.__sol_instr_09_partial_call(ethereum_transaction, ethereum_transaction.iterative_steps, data) 
            )

        self.__send_neon_transaction(ethereum_transaction, solana_trx)

        while True:
            solana_trx = Transaction().add(
                    self.__sol_instr_10_continue(ethereum_transaction, ethereum_transaction.iterative_steps) 
                )
            result = self.__send_neon_transaction(ethereum_transaction, solana_trx)

            if result['result']['meta']['innerInstructions'] \
                    and result['result']['meta']['innerInstructions'][0]['instructions']:
                data = base58.b58decode(result['result']['meta']['innerInstructions'][0]['instructions'][-1]['data'])
                if data[0] == 6:
                    ethereum_transaction.__storage = None
                    return result

    def send_ethereum_trx_single(self, ethereum_transaction) -> types.RPCResponse:
        assert (isinstance(ethereum_transaction, EthereumTransaction))
        (from_address, sign, msg) = self.__create_instruction_data_from_tx(ethereum_transaction)
        data = from_address + sign + msg
        keccak_data = make_keccak_instruction_data(1, len(msg), 5)

        solana_trx = Transaction().add(
                self.__sol_instr_keccak(keccak_data) 
            ).add( 
                self.__sol_instr_05(ethereum_transaction, data)
            )
        return self.__send_neon_transaction(ethereum_transaction, solana_trx)

    def __create_solana_ether_caller(self, ethereum_transaction):
        caller = self.evm_loader.ether2program(ethereum_transaction.ether_caller)[0]
        if ethereum_transaction._solana_ether_caller is None \
                or ethereum_transaction._solana_ether_caller != caller:
            ethereum_transaction._solana_ether_caller = caller
        if getBalance(ethereum_transaction._solana_ether_caller) == 0:
            print("Create solana ether caller account...")
            ethereum_transaction._solana_ether_caller = \
                self.evm_loader.createEtherAccount(ethereum_transaction.ether_caller)
            self.token.transfer(
                ETH_TOKEN_MINT_ID, 
                2000, 
                get_associated_token_address(PublicKey(caller), ETH_TOKEN_MINT_ID)
                )
        print("Solana ether caller account:", ethereum_transaction._solana_ether_caller)

    def __create_storage_account(self, seed):
        storage = PublicKey(
            sha256(bytes(self.solana_wallet.public_key())
                   + bytes(seed, 'utf8')
                   + bytes(PublicKey(self.evm_loader.loader_id))).digest())
        print("Storage", storage)

        if getBalance(storage) == 0:
            trx = Transaction()
            trx.add(createAccountWithSeed(self.solana_wallet.public_key(),
                                          self.solana_wallet.public_key(),
                                          seed, 10 ** 9, 128 * 1024,
                                          PublicKey(EVM_LOADER)))
            send_transaction(client, trx, self.solana_wallet)
        return storage

    def __create_instruction_data_from_tx(self, ethereum_transaction):
        self.__create_solana_ether_caller(ethereum_transaction)
        caller_trx_cnt = getTransactionCount(client, ethereum_transaction._solana_ether_caller)
        trx_raw = {'to': solana2ether(ethereum_transaction.contract_account),
                   'value': 0, 'gas': 9999999, 'gasPrice': 1_000_000_000, 'nonce': caller_trx_cnt,
                   'data': ethereum_transaction.trx_data, 'chainId': 111}
        return make_instruction_data_from_tx(trx_raw, self.solana_wallet.secret_key())

    def __sol_instr_keccak(self, keccak_data):
        return TransactionInstruction(
            program_id = PublicKey(keccakprog), 
            data = keccak_data, 
            keys = [
                AccountMeta(pubkey=PublicKey(keccakprog), is_signer=False, is_writable=False),
            ]
        )

    def __sol_instr_05(self, ethereum_transaction, data):
        return TransactionInstruction(
            program_id=self.evm_loader.loader_id,
            data=bytearray.fromhex("05") + self.collateral_pool_index_buf + data, 
            keys=[
                # Additional accounts for EvmInstruction::CallFromRawEthereumTX:
                # System instructions account:
                AccountMeta(pubkey=PublicKey(sysinstruct), is_signer=False, is_writable=False),
                # Operator address:
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=True, is_writable=True),
                # Collateral pool address:
                AccountMeta(pubkey=self.collateral_pool_address, is_signer=False, is_writable=True),
                # Operator ETH address (stub for now):
                AccountMeta(pubkey=get_associated_token_address(self.solana_wallet.public_key(), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                # User ETH address (stub for now):
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction._solana_ether_caller), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                # System program account:
                AccountMeta(pubkey=PublicKey(system), is_signer=False, is_writable=False),

                AccountMeta(pubkey=ethereum_transaction.contract_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction.contract_account), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction.contract_code_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction._solana_ether_caller, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction._solana_ether_caller), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),

                AccountMeta(pubkey=PublicKey(self.evm_loader.loader_id), is_signer=False, is_writable=False),
                AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=False, is_writable=False),
            ])

    def __sol_instr_09_partial_call(self, ethereum_transaction, step_count, data):
        return TransactionInstruction(
            program_id=self.evm_loader.loader_id,
            data=bytearray.fromhex("09") + self.collateral_pool_index_buf + step_count.to_bytes(8, byteorder='little') + data,
            keys=[
                AccountMeta(pubkey=ethereum_transaction._storage, is_signer=False, is_writable=True),

                # System instructions account:
                AccountMeta(pubkey=PublicKey(sysinstruct), is_signer=False, is_writable=False),
                # Operator address:
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=True, is_writable=True),
                # Collateral pool address:
                AccountMeta(pubkey=self.collateral_pool_address, is_signer=False, is_writable=True),
                # System program account:
                AccountMeta(pubkey=PublicKey(system), is_signer=False, is_writable=False),

                AccountMeta(pubkey=ethereum_transaction.contract_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction.contract_account), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction.contract_code_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction._solana_ether_caller, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction._solana_ether_caller), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),

                AccountMeta(pubkey=PublicKey(sysinstruct), is_signer=False, is_writable=False),
                AccountMeta(pubkey=PublicKey(self.evm_loader.loader_id), is_signer=False, is_writable=False),
                AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=False, is_writable=False),
            ])

    def __sol_instr_10_continue(self, ethereum_transaction, step_count):
        return TransactionInstruction(
            program_id=self.evm_loader.loader_id,
            data=bytearray.fromhex("0A") + step_count.to_bytes(8, byteorder='little'),
            keys=[
                AccountMeta(pubkey=ethereum_transaction._storage, is_signer=False, is_writable=True),

                # Operator address:
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=True, is_writable=True),
                # User ETH address (stub for now):
                AccountMeta(pubkey=get_associated_token_address(self.solana_wallet.public_key(), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                # User ETH address (stub for now):
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction._solana_ether_caller), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                # System program account:
                AccountMeta(pubkey=PublicKey(system), is_signer=False, is_writable=False),

                AccountMeta(pubkey=ethereum_transaction.contract_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction.contract_account), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction.contract_code_account, is_signer=False, is_writable=True),
                AccountMeta(pubkey=ethereum_transaction._solana_ether_caller, is_signer=False, is_writable=True),
                AccountMeta(pubkey=get_associated_token_address(PublicKey(ethereum_transaction._solana_ether_caller), ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),

                AccountMeta(pubkey=PublicKey(sysinstruct), is_signer=False, is_writable=False),
                AccountMeta(pubkey=PublicKey(self.evm_loader.loader_id), is_signer=False, is_writable=False),
                AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=self.solana_wallet.public_key(), is_signer=False, is_writable=False),
            ])

    def __send_neon_transaction(self, ethereum_transaction, trx) -> types.RPCResponse:
        if ethereum_transaction.trx_account_metas is not None:
            trx.instructions[-1].keys.extend(ethereum_transaction.trx_account_metas)

        return send_transaction(client, trx, self.solana_wallet)


def create_collateral_pool_address(collateral_pool_index):
    COLLATERAL_SEED_PREFIX = "collateral_seed_"
    seed = COLLATERAL_SEED_PREFIX + str(collateral_pool_index)
    return accountWithSeed(PublicKey(collateral_pool_base), seed, PublicKey(EVM_LOADER))


def confirm_transaction(http_client, tx_sig, confirmations=0):
    """Confirm a transaction."""
    TIMEOUT = 30  # 30 seconds pylint: disable=invalid-name
    elapsed_time = 0
    while elapsed_time < TIMEOUT:
        print('confirm_transaction for %s', tx_sig)
        resp = http_client.get_signature_statuses([tx_sig])
        print('confirm_transaction: %s', resp)
        if resp["result"]:
            status = resp['result']['value'][0]
            if status and (status['confirmationStatus'] == 'finalized' or status['confirmationStatus'] == 'confirmed'
                           and status['confirmations'] >= confirmations):
                return
        sleep_time = 0.1
        time.sleep(sleep_time)
        elapsed_time += sleep_time
    raise RuntimeError("could not confirm transaction: ", tx_sig)


def accountWithSeed(base, seed, program):
    print(type(base), type(seed), type(program))
    return PublicKey(sha256(bytes(base) + bytes(seed, 'utf8') + bytes(program)).digest())

def createAccountWithSeed(funding, base, seed, lamports, space, program):
    data = SYSTEM_INSTRUCTIONS_LAYOUT.build(
        dict(
            instruction_type=SystemInstructionType.CREATE_ACCOUNT_WITH_SEED,
            args=dict(
                base=bytes(base),
                seed=dict(length=len(seed), chars=seed),
                lamports=lamports,
                space=space,
                program_id=bytes(program)
            )
        )
    )
    print("createAccountWithSeed", data.hex())
    created = accountWithSeed(base, seed, program)
    print("created", created)
    return TransactionInstruction(
        keys=[
            AccountMeta(pubkey=funding, is_signer=True, is_writable=True),
            AccountMeta(pubkey=created, is_signer=False, is_writable=True),
            AccountMeta(pubkey=base, is_signer=True, is_writable=False),
        ],
        program_id=system,
        data=data
    )


class solana_cli:
    def __init__(self, acc=None):
        self.acc = acc

    def call(self, arguments):
        cmd = ""
        if self.acc == None:
            cmd = '{} --url {} {}'.format(path_to_solana, solana_url, arguments)
        else:
            cmd = '{} --keypair {} --url {} {}'.format(path_to_solana, self.acc.get_path(), solana_url, arguments)
        try:
            return subprocess.check_output(cmd, shell=True, universal_newlines=True)
        except subprocess.CalledProcessError as err:
            import sys
            print("ERR: solana error {}".format(err))
            raise


class neon_cli:
    def __init__(self, verbose_flags=''):
        self.verbose_flags = verbose_flags

    def call(self, arguments):
        cmd = 'neon-cli {} --url {} {} -vvv'.format(self.verbose_flags, solana_url, arguments)
        try:
            return subprocess.check_output(cmd, shell=True, universal_newlines=True)
        except subprocess.CalledProcessError as err:
            import sys
            print("ERR: neon-cli error {}".format(err))
            raise

    def emulate(self, loader_id, arguments):
        cmd = 'neon-cli {} --commitment=recent --evm_loader {} --url {} emulate {}'.format(self.verbose_flags,
                                                                                           loader_id,
                                                                                           solana_url,
                                                                                           arguments)
        print('cmd:', cmd)
        try:
            output = subprocess.check_output(cmd, shell=True, universal_newlines=True)
            without_empty_lines = os.linesep.join([s for s in output.splitlines() if s])
            last_line = without_empty_lines.splitlines()[-1]
            return last_line
        except subprocess.CalledProcessError as err:
            import sys
            print("ERR: neon-cli error {}".format(err))
            raise


class RandomAccount:
    def __init__(self, path=None):
        if path == None:
            self.make_random_path()
            print("New keypair file: {}".format(self.path))
            self.generate_key()
        else:
            self.path = path
        self.retrieve_keys()
        print('New Public key:', self.acc.public_key())
        print('Private:', self.acc.secret_key())

    def make_random_path(self):
        self.path  = os.urandom(5).hex()+ ".json"

    def generate_key(self):
        cmd_generate = 'solana-keygen new --no-passphrase --outfile {}'.format(self.path)
        try:
            return subprocess.check_output(cmd_generate, shell=True, universal_newlines=True)
        except subprocess.CalledProcessError as err:
            import sys
            print("ERR: solana error {}".format(err))
            raise

    def retrieve_keys(self):
        with open(self.path) as f:
            d = json.load(f)
            self.acc = Account(d[0:32])

    def get_path(self):
        return self.path

    def get_acc(self):
        return self.acc


class WalletAccount(RandomAccount):
    def __init__(self, path):
        self.path = path
        self.retrieve_keys()
        print('Wallet public key:', self.acc.public_key())


class EvmLoader:
    def __init__(self, acc, programId=EVM_LOADER):
        if programId == None:
            print("Load EVM loader...")
            result = json.loads(solana_cli(acc).call('deploy {}'.format(EVM_LOADER_SO)))
            programId = result['programId']
        EvmLoader.loader_id = programId
        print("Done\n")

        self.loader_id = EvmLoader.loader_id
        self.acc = acc
        print("Evm loader program: {}".format(self.loader_id))

    def deploy(self, contract_path, config=None):
        print('deploy contract')
        if config == None:
            output = neon_cli().call("deploy --evm_loader {} {}".format(self.loader_id, contract_path))
        else:
            output = neon_cli().call("deploy --evm_loader {} --config {} {}".format(self.loader_id, config,
                                                                                       contract_path))
        print(type(output), output)
        result = json.loads(output.splitlines()[-1])
        return result

    def createEtherAccount(self, ether):
        if isinstance(ether, str):
            if ether.startswith('0x'): ether = ether[2:]
        else:
            ether = ether.hex()
        (sol, nonce) = self.ether2program(ether)
        print('createEtherAccount: {} {} => {}'.format(ether, nonce, sol))
        associated_token = get_associated_token_address(PublicKey(sol), ETH_TOKEN_MINT_ID)
        trx = Transaction()
        base = self.acc.get_acc().public_key()
        trx.add(TransactionInstruction(
            program_id=self.loader_id,
            data=bytes.fromhex('02000000') + CREATE_ACCOUNT_LAYOUT.build(dict(
                lamports=10 ** 9,
                space=0,
                ether=bytes.fromhex(ether),
                nonce=nonce)),
            keys=[
                AccountMeta(pubkey=base, is_signer=True, is_writable=False),
                AccountMeta(pubkey=PublicKey(sol), is_signer=False, is_writable=True),
                AccountMeta(pubkey=associated_token, is_signer=False, is_writable=True),
                AccountMeta(pubkey=system, is_signer=False, is_writable=False),
                AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=ASSOCIATED_TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                AccountMeta(pubkey=rentid, is_signer=False, is_writable=False),
            ]))
        result = send_transaction(client, trx, self.acc.get_acc())
        print('result:', result)
        return sol

    def ether2seed(self, ether):
        if isinstance(ether, str):
            if ether.startswith('0x'): ether = ether[2:]
        else:
            ether = ether.hex()
        seed = b58encode(bytes.fromhex(ether)).decode('utf8')
        acc = accountWithSeed(self.acc.get_acc().public_key(), seed, PublicKey(self.loader_id))
        print('ether2program: {} {} => {}'.format(ether, 255, acc))
        return (acc, 255)

    def ether2program(self, ether):
        if isinstance(ether, str):
            if ether.startswith('0x'): ether = ether[2:]
        else:
            ether = ether.hex()
        output = neon_cli().call("create-program-address --evm_loader {} {}".format(self.loader_id, ether))
        items = output.rstrip().split(' ')
        return items[0], int(items[1])

    def checkAccount(self, solana):
        info = client.get_account_info(solana)
        print("checkAccount({}): {}".format(solana, info))

    def deployChecked(self, location, caller, caller_ether):
        trx_count = getTransactionCount(client, caller)
        ether = keccak_256(rlp.encode((caller_ether, trx_count))).digest()[-20:]

        program = self.ether2program(ether)
        code = self.ether2seed(ether)
        info = client.get_account_info(program[0])
        if info['result']['value'] is None:
            res = self.deploy(location)
            return res['programId'], bytes.fromhex(res['ethereum'][2:]), res['codeId']
        elif info['result']['value']['owner'] != self.loader_id:
            raise Exception("Invalid owner for account {}".format(program))
        else:
            return program[0], ether, code[0]

    def createEtherAccountTrx(self, ether, code_acc=None):
        if isinstance(ether, str):
            if ether.startswith('0x'): ether = ether[2:]
        else:
            ether = ether.hex()
        (sol, nonce) = self.ether2program(ether)
        token = get_associated_token_address(PublicKey(sol), ETH_TOKEN_MINT_ID)
        print('createEtherAccount: {} {} => {}'.format(ether, nonce, sol))
        seed = b58encode(bytes.fromhex(ether))
        base = self.acc.get_acc().public_key()
        data = bytes.fromhex('02000000') + CREATE_ACCOUNT_LAYOUT.build(dict(
            lamports=10 ** 9,
            space=0,
            ether=bytes.fromhex(ether),
            nonce=nonce))
        trx = Transaction()
        if code_acc is None:
            trx.add(TransactionInstruction(
                program_id=self.loader_id,
                data=data,
                keys=[
                    AccountMeta(pubkey=base, is_signer=True, is_writable=True),
                    AccountMeta(pubkey=PublicKey(sol), is_signer=False, is_writable=True),
                    AccountMeta(pubkey=token, is_signer=False, is_writable=True),
                    AccountMeta(pubkey=system, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=ASSOCIATED_TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=rentid, is_signer=False, is_writable=False),
                ]))
        else:
            trx.add(TransactionInstruction(
                program_id=self.loader_id,
                data=data,
                keys=[
                    AccountMeta(pubkey=base, is_signer=True, is_writable=True),
                    AccountMeta(pubkey=PublicKey(sol), is_signer=False, is_writable=True),
                    AccountMeta(pubkey=token, is_signer=False, is_writable=True),
                    AccountMeta(pubkey=PublicKey(code_acc), is_signer=False, is_writable=True),
                    AccountMeta(pubkey=system, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=ETH_TOKEN_MINT_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=ASSOCIATED_TOKEN_PROGRAM_ID, is_signer=False, is_writable=False),
                    AccountMeta(pubkey=rentid, is_signer=False, is_writable=False),
                ]))
        return (trx, sol)


def create_with_seed_loader_instruction(evm_loader_id, funding, created, base, seed, lamports, space, owner):
    return TransactionInstruction(
        program_id=evm_loader_id,
        data=bytes.fromhex("04000000") + \
            bytes(base) + \
            len(seed).to_bytes(8, byteorder='little') + \
            bytes(seed, 'utf8') + \
            lamports.to_bytes(8, byteorder='little') + \
            space.to_bytes(8, byteorder='little') + \
            bytes(owner) + \
            bytes(created),
        keys=[
            AccountMeta(pubkey=funding, is_signer=True, is_writable=False),
            AccountMeta(pubkey=created, is_signer=False, is_writable=True),
            AccountMeta(pubkey=base, is_signer=False, is_writable=True),
            AccountMeta(pubkey=created, is_signer=False, is_writable=True),
            AccountMeta(pubkey=PublicKey(evm_loader_id), is_signer=False, is_writable=True),
            AccountMeta(pubkey=PublicKey(ETH_TOKEN_MINT_ID), is_signer=False, is_writable=True),
            AccountMeta(pubkey=PublicKey(tokenkeg), is_signer=False, is_writable=True),
            AccountMeta(pubkey=PublicKey(rentid), is_signer=False, is_writable=True),
            AccountMeta(pubkey=PublicKey(system), is_signer=False, is_writable=True),
        ])


def getBalance(account):
    return client.get_balance(account, commitment=Confirmed)['result']['value']


def solana2ether(public_key):
    from web3 import Web3
    return bytes(Web3.keccak(bytes(PublicKey(public_key)))[-20:])


ACCOUNT_INFO_LAYOUT = cStruct(
    "type" / Int8ul,
    "eth_acc" / Bytes(20),
    "nonce" / Int8ul,
    "trx_count" / Bytes(8),
    "code_acc" / Bytes(32),
    "is_blocked" / Int8ul,
    "blocked_by" / Bytes(32),
    "eth_token" / Bytes(32),
)


class AccountInfo(NamedTuple):
    eth_acc: eth_keys.PublicKey
    trx_count: int

    @staticmethod
    def frombytes(data):
        cont = ACCOUNT_INFO_LAYOUT.parse(data)
        return AccountInfo(cont.eth_acc, cont.trx_count)


def getAccountData(client, account, expected_length):
    info = client.get_account_info(account, commitment=Confirmed)['result']['value']
    if info is None:
        raise Exception("Can't get information about {}".format(account))

    data = base64.b64decode(info['data'][0])
    if len(data) < expected_length:
        print("len(data)({}) < expected_length({})".format(len(data), expected_length))
        raise Exception("Wrong data length for account data {}".format(account))
    return data


def getTransactionCount(client, sol_account):
    info = getAccountData(client, sol_account, ACCOUNT_INFO_LAYOUT.sizeof())
    acc_info = AccountInfo.frombytes(info)
    res = int.from_bytes(acc_info.trx_count, 'little')
    print('getTransactionCount {}: {}'.format(sol_account, res))
    return res


def wallet_path():
    res = solana_cli().call("config get")
    substr = "Keypair Path: "
    for line in res.splitlines():
        if line.startswith(substr):
            return line[len(substr):].strip()
    raise Exception("cannot get keypair path")


def send_transaction(client, trx, acc):
    result = client.send_transaction(trx, acc, opts=TxOpts(skip_confirmation=True, preflight_commitment="confirmed"))
    confirm_transaction(client, result["result"])
    result = client.get_confirmed_transaction(result["result"])
    return result
