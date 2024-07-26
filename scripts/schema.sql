create table new_deposit_requests (
    id INTEGER primary key autoincrement, 
    start_utxo text not null,
    recovery_taproot_address text not null,
    evm_address text not null check (length(evm_address) = 40 and evm_address glob '[a-fA-F0-9]*'),
    created_at timestamp not null default current_timestamp
);

create table deposit_move_txs (
    id INTEGER primary key autoincrement,
    start_utxo text not null,
    recovery_taproot_address text not null,
    evm_address text not null check (length(evm_address) = 40 and evm_address glob '[a-fA-F0-9]*'),
    move_txid text not null unique check (length(move_txid) = 64 and move_txid glob '[a-fA-F0-9]*'),
    created_at timestamp not null default current_timestamp
);

create table withdrawal_sigs (
    idx INTEGER primary key autoincrement,
    bridge_fund_txid text not null check (length(bridge_fund_txid) = 64 and bridge_fund_txid glob '[a-fA-F0-9]*'),
    sig text not null check (length(sig) = 128 and sig glob '[a-fA-F0-9]*'),
    created_at timestamp not null default current_timestamp
);
