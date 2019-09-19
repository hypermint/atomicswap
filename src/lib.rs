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

#[contract]
pub fn open_swap() -> R<i32> {
    let sender = api::get_sender()?;
    let swap_id: Vec<u8> = api::get_arg(0)?;
    let open_value: u64 = api::get_arg(1)?;
    // ERC20
    let open_contract_address: Address = api::get_arg(2)?;
    let close_value: u64 = api::get_arg(3)?;
    let close_trader: Address = api::get_arg(4)?;
    // ERC721
    let close_contract_address: Address = api::get_arg(5)?;

    let state = get_swap_states(&swap_id)?;
    if state != States::NONE {
        return Err(error::from_str("this swap_id already exists"));
    }

    // open-contract transfer to this contract
    let _: Vec<u8> = api::call_contract(
        &open_contract_address,
        b"transferFrom",
        vec![
            &sender.to_bytes(),
            &api::get_contract_address()?.to_bytes(),
            &open_value.to_bytes(),
        ],
    )?;

    let swap = Swap {
        open_value,
        open_trader: sender,
        open_contract_address,
        close_value,
        close_trader,
        close_contract_address,
    };

    set_swap(&swap_id, &swap)?;
    set_swap_states(&swap_id, States::OPEN);

    Ok(None)
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Swap {
    open_value: u64,
    open_trader: Address,
    open_contract_address: Address,
    close_value: u64,
    close_trader: Address,
    close_contract_address: Address,
}

#[contract]
pub fn get_swap_info() -> R<Vec<u8>> {
    let swap_id: Vec<u8> = api::get_arg(0)?;
    let swap = get_swap(&swap_id)?;
    Ok(Some(json::serialize(&swap)?))
}

#[contract]
pub fn get_swap_status() -> R<u8> {
    let swap_id: Vec<u8> = api::get_arg(0)?;
    match get_swap_states(&swap_id) {
        Ok(s) => Ok(Some(s as u8)),
        Err(e) => Err(e),
    }
}

#[contract]
pub fn cancel_swap() -> R<u32> {
    let sender = api::get_sender()?;
    let swap_id: Vec<u8> = api::get_arg(0)?;
    check_swap_open(&swap_id)?;
    let swap = get_swap(&swap_id)?;
    if swap.open_trader != sender {
        Err(error::from_str("unexpected sender"))
    } else {
        set_swap_states(&swap_id, States::CANCELED);
        Ok(None)
    }
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
        b"transferFrom",
        vec![
            &sender.to_bytes(),
            &swap.open_trader.to_bytes(),
            &swap.close_value.to_bytes(),
        ],
    )?;
    let _: Vec<u8> = api::call_contract(
        &swap.open_contract_address,
        b"transfer",
        vec![&swap.close_trader.to_bytes(), &swap.open_value.to_bytes()],
    )?;

    Ok(None)
}

fn check_swap_open(swap_id: &[u8]) -> Result<(), Error> {
    match get_swap_states(swap_id) {
        Ok(States::OPEN) => Ok(()),
        s => Err(error::from_str(format!(
            "swap state must be OPEN, but got {:?}",
            s
        ))),
    }
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

#[derive(Debug, PartialEq)]
#[repr(u8)]
enum States {
    NONE,
    OPEN,
    CLOSED,
    CANCELED,
}

fn state_from_u8(n: u8) -> Option<States> {
    use States::*;
    match n {
        0 => Some(NONE),
        1 => Some(OPEN),
        2 => Some(CLOSED),
        3 => Some(CANCELED),
        _ => None,
    }
}

fn set_swap_states(swap_id: &[u8], state: States) {
    let key = make_swap_states_key(swap_id);
    api::write_state(&key, &[state as u8])
}

fn get_swap_states(swap_id: &[u8]) -> Result<States, Error> {
    let key = make_swap_states_key(swap_id);
    match api::read_state::<Vec<u8>>(&key) {
        Ok(v) => match state_from_u8(v[0]) {
            Some(s) => Ok(s),
            None => Err(error::from_str("invalid state")),
        },
        Err(_) => Ok(States::NONE),
    }
}

fn make_swaps_key(swap_id: &[u8]) -> Vec<u8> {
    make_key_by_parts(vec![b"swaps", swap_id])
}

fn make_swap_states_key(swap_id: &[u8]) -> Vec<u8> {
    make_key_by_parts(vec![b"swapStates", swap_id])
}

fn make_key_by_parts(parts: Vec<&[u8]>) -> Vec<u8> {
    parts.join(&b'/')
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
    const CONTRACT_SWAP: Address = *b"00000000000000000100";
    const CONTRACT_TOKEN_OPEN: Address = *b"00000000000000000101";
    const CONTRACT_TOKEN_CLOSE: Address = *b"00000000000000000110";
    const TOKEN1: u64 = 1;

    #[test]
    fn init_test() {
        let _ =
            hmemu::run_process(|| hmemu::call_contract(&SENDER1, vec![], || Ok(init()))).unwrap();
    }

    fn setup_swap(swap_id: Vec<u8>) -> Result<()> {
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
                args.push(100u64);
                args.convert_to_vec()
            };
            hmemu::call_contract(&SENDER1, args, || erc20::approve())?;
        }
        {
            // ensure that our swap id is unused
            hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
            let args = {
                let mut args = ArgsBuilder::new();
                args.push(swap_id.clone()); // swap_id
                args.convert_to_vec()
            };
            hmemu::call_contract(&SENDER1, args, || {
                let status = get_swap_status()?;
                assert_eq!(Some(0u8), status);
                Ok(())
            })?;
        }
        {
            // open a swap contract. (sender1 is opener)
            hmemu::init_contract_address(&CONTRACT_SWAP)?;
            let args = {
                let mut args = ArgsBuilder::new();
                args.push(swap_id.clone()); // swap_id
                args.push(100u64); // open_value
                args.push(CONTRACT_TOKEN_OPEN); // open_contract
                args.push(TOKEN1); // close_value(tokenID)
                args.push(SENDER2); // close_trader
                args.push(CONTRACT_TOKEN_CLOSE); // close_contract
                args.convert_to_vec()
            };
            hmemu::call_contract(&SENDER1, args, || open_swap())?;
        }
        {
            // check if swap status is valid
            hmemu::init_contract_address(&CONTRACT_SWAP)?;
            let args = {
                let mut args = ArgsBuilder::new();
                args.push(swap_id.clone()); // swap_id
                args.convert_to_vec()
            };
            hmemu::call_contract(&SENDER1, args, || {
                let status = get_swap_status()?.unwrap();
                assert_eq!(Some(States::OPEN), state_from_u8(status));
                Ok(())
            })?;
        }
        {
            // check if swap info is valid
            hmemu::init_contract_address(&CONTRACT_SWAP)?;
            let args = {
                let mut args = ArgsBuilder::new();
                args.push(swap_id.clone()); // swap_id
                args.convert_to_vec()
            };
            hmemu::call_contract(&SENDER1, args, || {
                let swap_bytes = get_swap_info()?.unwrap();
                let swap: Swap = json::deserialize(&swap_bytes)?;
                assert_eq!(
                    swap,
                    Swap {
                        open_value: 100u64,
                        open_trader: SENDER1,
                        open_contract_address: CONTRACT_TOKEN_OPEN,
                        close_value: TOKEN1,
                        close_trader: SENDER2,
                        close_contract_address: CONTRACT_TOKEN_CLOSE,
                    }
                );
                Ok(())
            })?;
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
        Ok(())
    }

    #[test]
    fn standard_swap_test() {
        let swap_id = b"swap1".to_vec();
        hmemu::run_process(|| {
            setup_swap(swap_id.clone())?;
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

    #[test]
    fn cancel_swap_test() {
        let swap_id = b"swap2".to_vec();
        hmemu::run_process(|| {
            setup_swap(swap_id.clone())?;
            {
                // ensure that canceling swap is success before closing
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    let _ = cancel_swap()?;
                    Ok(())
                })?;
            }
            {
                // check if swap status is valid
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    let status = get_swap_status()?.unwrap();
                    assert_eq!(Some(States::CANCELED), state_from_u8(status));
                    Ok(())
                })?;
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn try_cancel_after_close() {
        let swap_id = b"swap3".to_vec();
        hmemu::run_process(|| {
            setup_swap(swap_id.clone())?;
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
                // ensure that canceling swap is failed after closing
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    assert!(cancel_swap().is_err());
                    Ok(())
                })?;
            }
            {
                // check if swap status is valid
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    let status = get_swap_status()?.unwrap();
                    assert_eq!(Some(States::CLOSED), state_from_u8(status));
                    Ok(())
                })?;
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn duplicated_closing() {
        let swap_id = b"swap4".to_vec();
        hmemu::run_process(|| {
            setup_swap(swap_id.clone())?;
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
                // close swap contract again. (sender2 is closer)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone());
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER2, args, || {
                    assert!(close_swap().is_err());
                    Ok(())
                })?;
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn open_duplicated_swaps() {
        let swap_id = b"swap5".to_vec();
        hmemu::run_process(|| {
            setup_swap(swap_id.clone())?;
            {
                // open a swap contract with same swap_id. (sender1 is opener)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.push(50u64); // open_value
                    args.push(CONTRACT_TOKEN_OPEN); // open_contract
                    args.push(TOKEN1); // close_value(tokenID)
                    args.push(SENDER2); // close_trader
                    args.push(CONTRACT_TOKEN_CLOSE); // close_contract
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    assert!(open_swap().is_err());
                    Ok(())
                })?;
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
                // open a swap contract with same swap_id again. (sender1 is opener)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let args = {
                    let mut args = ArgsBuilder::new();
                    args.push(swap_id.clone()); // swap_id
                    args.push(10u64); // open_value
                    args.push(CONTRACT_TOKEN_OPEN); // open_contract
                    args.push(TOKEN1); // close_value(tokenID)
                    args.push(SENDER2); // close_trader
                    args.push(CONTRACT_TOKEN_CLOSE); // close_contract
                    args.convert_to_vec()
                };
                hmemu::call_contract(&SENDER1, args, || {
                    assert!(open_swap().is_err());
                    Ok(())
                })?;
            }
            Ok(())
        })
        .unwrap();
    }
}
