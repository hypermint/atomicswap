extern crate hmcdk;
use hmcdk::api;
use hmcdk::error;
use hmcdk::prelude::*;
#[macro_use]
extern crate serde;
mod json;

#[contract]
pub fn init() -> R<i32> {
    Ok(None)
}

pub fn open_swap() -> R<i32> {
    let sender = api::get_sender()?;
    println!(
        "OPEN_SWAP sender={:X?} contract_address={:X?}",
        sender,
        api::get_contract_address()?
    );
    let swap_id: Vec<u8> = api::get_arg(0)?;
    let open_value: u64 = api::get_arg(1)?;
    // ERC20
    let open_contract: Address = api::get_arg(2)?;
    let close_value: u64 = api::get_arg(3)?;
    let close_trader: Address = api::get_arg(4)?;
    // ERC721
    let close_contract: Address = api::get_arg(5)?;

    // open-contract transfer to this contract
    let _: Vec<u8> = api::call_contract(
        &open_contract,
        "transferFrom".as_bytes(),
        vec![
            &sender.to_bytes(),
            &api::get_contract_address()?.to_bytes(),
            &open_value.to_bytes(),
        ],
    )?;

    let swap = Swap {
        open_value: open_value,
        open_trader: sender,
        open_contract_address: open_contract,
        close_value: close_value,
        close_trader: close_trader,
        close_contract_address: close_contract,
    };

    set_swap(&swap_id, &swap)?;
    set_swap_states(&swap_id, States::OPEN);

    Ok(None)
}

#[derive(Serialize, Deserialize, Debug)]
struct Swap {
    open_value: u64,
    open_trader: Address,
    open_contract_address: Address,
    close_value: u64,
    close_trader: Address,
    close_contract_address: Address,
}

#[contract]
pub fn close_swap() -> R<i32> {
    let sender = api::get_sender()?; // this means closer address
    let swap_id: Vec<u8> = api::get_arg(0)?;
    check_swap_open(&swap_id)?;

    let swap = get_swap(&swap_id)?;
    set_swap_states(&swap_id, States::CLOSED);

    let _: Vec<u8> = api::call_contract(
        &swap.close_contract_address,
        "transferFrom".as_bytes(),
        vec![
            &sender.to_bytes(),
            &swap.open_trader.to_bytes(),
            &swap.close_value.to_bytes(),
        ],
    )?;
    let _: Vec<u8> = api::call_contract(
        &swap.open_contract_address,
        "transfer".as_bytes(),
        vec![&swap.close_trader.to_bytes(), &swap.open_value.to_bytes()],
    )?;

    Ok(None)
}

fn check_swap_open(swap_id: &[u8]) -> Result<(), Error> {
    match get_swap_states(swap_id) {
        Some(States::OPEN) => Ok(()),
        s => Err(error::from_str(format!(
            "swap state must be OPEN, but got {:?}",
            s
        ))),
    }
}

fn bytes_to_hex_string(b: &[u8]) -> String {
    let mut w = String::with_capacity(b.as_ref().len() * 2 + 2);
    static CHARS: &'static [u8] = b"0123456789abcdef";

    w.push_str("0x");
    for &byte in b.as_ref().iter() {
        w.push(CHARS[(byte >> 4) as usize].into());
        w.push(CHARS[(byte & 0xf) as usize].into());
    }

    w
}

fn set_swap(swap_id: &[u8], swap: &Swap) -> Result<(), Error> {
    let b = json::serialize(swap)?;
    let key = make_swaps_key(swap_id);
    api::write_state(&key, &b);
    Ok(())
}

fn get_swap(swap_id: &[u8]) -> Result<Swap, Error> {
    let key = make_swaps_key(swap_id);
    let b: Vec<u8> = api::read_state(&key)?;
    json::deserialize(&b)
}

#[derive(Debug)]
#[repr(u8)]
enum States {
    INVALID,
    OPEN,
    CLOSED,
    EXPIRED,
}

fn state_from_u8(n: u8) -> Option<States> {
    use States::*;
    match n {
        0 => Some(INVALID),
        1 => Some(OPEN),
        2 => Some(CLOSED),
        3 => Some(EXPIRED),
        _ => None,
    }
}

fn set_swap_states(swap_id: &[u8], state: States) {
    let key = make_swap_states_key(swap_id);
    api::write_state(&key, &[state as u8])
}

fn get_swap_states(swap_id: &[u8]) -> Option<States> {
    let key = make_swap_states_key(swap_id);
    match api::read_state::<Vec<u8>>(&key) {
        Ok(v) => state_from_u8(v[0]),
        Err(_) => None,
    }
}

fn make_swaps_key(swap_id: &[u8]) -> Vec<u8> {
    make_key_by_parts(vec!["swaps".as_bytes(), swap_id])
}

fn make_swap_states_key(swap_id: &[u8]) -> Vec<u8> {
    make_key_by_parts(vec!["swapStates".as_bytes(), swap_id])
}

fn make_key_by_parts(parts: Vec<&[u8]>) -> Vec<u8> {
    parts.join(&('/' as u8))
}

#[cfg(test)]
mod tests {
    extern crate erc20;
    extern crate erc721;
    extern crate hmemu;
    use super::*;
    use hmemu::types::ArgsBuilder;
    use hmemu::*;

    const SENDER1: Address = *b"00000000000000000001";
    const SENDER2: Address = *b"00000000000000000002";

    #[test]
    fn init_test() {
        let _ =
            hmemu::run_process(|| hmemu::call_contract(&SENDER1, vec![], || Ok(init()))).unwrap();
    }

    #[test]
    fn swap_test() {
        const CONTRACT_SWAP: Address = *b"00000000000000000100";
        const CONTRACT_TOKEN_OPEN: Address = *b"00000000000000000101";
        const CONTRACT_TOKEN_CLOSE: Address = *b"00000000000000000110";
        const TOKEN1: u64 = 1;
        let swap_id = b"swap1".to_vec();

        hmemu::run_process(|| {
            hmemu::register_contract_function(
                CONTRACT_TOKEN_OPEN,
                "transfer".to_string(),
                contract_fn!(erc20::transfer),
            );
            hmemu::register_contract_function(
                CONTRACT_TOKEN_OPEN,
                "transferFrom".to_string(),
                contract_fn!(erc20::transferFrom),
            );
            hmemu::register_contract_function(
                CONTRACT_TOKEN_CLOSE,
                "transferFrom".to_string(),
                contract_fn!(erc721::transferFrom),
            );

            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                hmemu::call_contract(&SENDER1, vec![], || Ok(erc20::init()?))?;
            }
            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;
                hmemu::call_contract(&SENDER2, vec![], || Ok(erc721::init()?))?;

                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(SENDER2);
                    args.push(TOKEN1);
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER2, args, || Ok(erc721::mint()?))?;
            }
            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                hmemu::call_contract(&SENDER1, vec![], || {
                    let balance = erc20::balanceOf()?.unwrap();
                    assert_eq!(100000 * 10, balance);
                    Ok(())
                })?;
            }
            {
                // approve a token on open-contract
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(CONTRACT_SWAP);
                    args.push(100i64);
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || erc20::approve())?;
            }

            {
                // open a swap contract. (sender1 is opener)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.push(100i64); // open_value
                    args.push(CONTRACT_TOKEN_OPEN); // open_contract
                    args.push(TOKEN1); // close_value(tokenID)
                    args.push(SENDER2); // close_trader
                    args.push(CONTRACT_TOKEN_CLOSE); // close_contract
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || open_swap())?;
            }
            {
                // approve a token on close-contract
                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(CONTRACT_SWAP);
                    args.push(TOKEN1);
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER2, args, || erc721::approve())?;
            }
            {
                // close swap contract. (sender2 is closer)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone());
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER2, args, || close_swap())?;
            }
            {
                // check if each balance is valid
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                hmemu::call_contract(&SENDER1, vec![], || {
                    assert_eq!(Some(100000 * 10 - 100), erc20::balanceOf()?);
                    Ok(())
                })?;
                hmemu::call_contract(&SENDER2, vec![], || {
                    assert_eq!(Some(100), erc20::balanceOf()?);
                    Ok(())
                })?;

                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(TOKEN1);
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    let owner = erc721::ownerOf()?.unwrap();
                    assert_eq!(SENDER1, owner);
                    Ok(())
                })?;
            }

            Ok(())
        })
        .unwrap();
    }
}
