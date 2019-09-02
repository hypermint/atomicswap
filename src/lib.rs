extern crate hmc;
#[macro_use]
extern crate serde;
mod json;

#[cfg_attr(not(feature = "emulation"), no_mangle)]
pub fn init() -> i32 {
    0
}

#[cfg_attr(not(feature = "emulation"), no_mangle)]
pub fn open_swap() -> i32 {
    match _open_swap() {
        Ok(_) => 0,
        Err(e) => {
            hmc::revert(e);
            -1
        }
    }
}

type Address = [u8; 20];

#[derive(Serialize, Deserialize, Debug)]
struct Swap {
    open_value: u64,
    open_trader: Address,
    open_contract_address: Address,
    close_value: u64,
    close_trader: Address,
    close_contract_address: Address,
}

fn _open_swap() -> Result<(), String> {
    let sender = hmc::get_sender()?;
    let swap_id = hmc::get_arg(0)?;
    let open_value = hmc::get_arg_str(1)?.parse::<u64>().unwrap();
    // ERC20
    let open_contract = hmc::hex_to_bytes(hmc::get_arg_str(2)?.as_ref());
    let close_value = hmc::get_arg_str(3)?.parse::<u64>().unwrap();
    let close_trader = hmc::hex_to_bytes(hmc::get_arg_str(4)?.as_ref());
    // ERC721
    let close_contract = hmc::hex_to_bytes(hmc::get_arg_str(5)?.as_ref());

    // open-contract transfer to this contract
    hmc::call_contract(
        &open_contract,
        "transferFrom".as_bytes(),
        vec![
            bytes_to_hex_string(&sender).as_bytes(),
            bytes_to_hex_string(&hmc::get_contract_address()?).as_bytes(),
            format!("{}", open_value).as_bytes(),
        ],
    )?;

    let swap = Swap {
        open_value: open_value,
        open_trader: sender,
        open_contract_address: slice_to_address(&open_contract)?,
        close_value: close_value,
        close_trader: slice_to_address(&close_trader)?,
        close_contract_address: slice_to_address(&close_contract)?,
    };

    set_swap(&swap_id, &swap)?;
    set_swap_states(&swap_id, States::OPEN);

    Ok(())
}

#[cfg_attr(not(feature = "emulation"), no_mangle)]
pub fn close_swap() -> i32 {
    match _close_swap() {
        Ok(_) => 0,
        Err(e) => {
            hmc::revert(e);
            -1
        }
    }
}

fn _close_swap() -> Result<(), String> {
    let sender = hmc::get_sender()?; // this equals closer
    let swap_id = hmc::get_arg(0)?;
    is_swap_open(&swap_id)?;

    let swap = get_swap(&swap_id)?;
    set_swap_states(&swap_id, States::CLOSED);

    hmc::call_contract(
        &swap.close_contract_address,
        "transferFrom".as_bytes(),
        vec![
            bytes_to_hex_string(&sender).as_bytes(),
            bytes_to_hex_string(&swap.open_trader).as_bytes(),
            format!("{}", swap.close_value).as_bytes(),
        ],
    )?;
    hmc::call_contract(
        &swap.open_contract_address,
        "transfer".as_bytes(),
        vec![
            bytes_to_hex_string(&swap.close_trader).as_bytes(),
            format!("{}", swap.open_value).as_bytes(),
        ],
    )?;

    Ok(())
}

fn is_swap_open(swap_id: &[u8]) -> Result<(), String> {
    match get_swap_states(swap_id) {
        Some(States::OPEN) => Ok(()),
        s => Err(format!("swap state must be OPEN, but got {:?}", s)),
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

fn slice_to_address(s: &[u8]) -> Result<Address, String> {
    if s.len() != 20 {
        Err(format!("invalid byte length: {}", s.len()))
    } else {
        let mut addr: Address = Default::default();
        addr.copy_from_slice(s);
        Ok(addr)
    }
}

fn set_swap(swap_id: &[u8], swap: &Swap) -> Result<(), String> {
    let b = json::serialize(swap)?;
    let key = make_swaps_key(swap_id);
    hmc::write_state(&key, &b);
    Ok(())
}

fn get_swap(swap_id: &[u8]) -> Result<Swap, String> {
    let key = make_swaps_key(swap_id);
    let b = hmc::read_state(&key)?;

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
    hmc::write_state(&key, &[state as u8])
}

fn get_swap_states(swap_id: &[u8]) -> Option<States> {
    let key = make_swap_states_key(swap_id);
    match hmc::read_state(&key) {
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
    use super::*;

    const SENDER1_ADDR: &str = "0x1221a0726d56aedea9dbe2522ddae3dd8ed0f36c";
    const SENDER2_ADDR: &str = "0xd8eba1f372b9e0d378259f150d52c2e6c2e4109a";

    #[test]
    fn it_works() {
        let sender = hmc::hex_to_bytes(SENDER1_ADDR);

        hmemu::run_process(|| hmemu::call_contract(&sender, Vec::<String>::new(), || Ok(init())))
            .unwrap();
    }

    #[test]
    fn hex_string_test() {
        let sender1 = hmc::hex_to_bytes(SENDER1_ADDR);
        let s1 = bytes_to_hex_string(&sender1);
        assert_eq!(SENDER1_ADDR, s1);
    }

    #[test]
    fn swap_test() {
        let sender1 = hmc::hex_to_bytes(SENDER1_ADDR);
        let sender2 = hmc::hex_to_bytes(SENDER2_ADDR);

        const CONTRACT_SWAP: Address = *b"00000000000000000001";
        const CONTRACT_TOKEN_OPEN: Address = *b"00000000000000000010";
        const CONTRACT_TOKEN_CLOSE: Address = *b"00000000000000000011";
        const TOKEN1: &str = "1";

        hmemu::run_process(|| {
            hmemu::register_contract_function(
                CONTRACT_TOKEN_OPEN,
                "transfer".to_string(),
                erc20::transfer,
            );
            hmemu::register_contract_function(
                CONTRACT_TOKEN_OPEN,
                "transferFrom".to_string(),
                erc20::transferFrom,
            );
            hmemu::register_contract_function(
                CONTRACT_TOKEN_CLOSE,
                "transferFrom".to_string(),
                erc721::transferFrom,
            );

            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                hmemu::call_contract(&sender1, Vec::<String>::new(), || Ok(erc20::init()))?;
            }
            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;
                hmemu::call_contract(&sender2, Vec::<String>::new(), || Ok(erc721::init()))?;
                hmemu::call_contract(&sender2, vec![SENDER2_ADDR, TOKEN1], || {
                    assert_eq!(0, erc721::mint());
                    Ok(())
                })?;
            }
            {
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                hmemu::call_contract(&sender1, Vec::<String>::new(), || {
                    assert_eq!(0, erc20::balanceOf());
                    let balance = hmemu::get_return_value()?;
                    assert_eq!(100000 * 10, bytes_to_i64(&balance));
                    Ok(())
                })?;
            }
            { // approve a token on open-contract
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;
                let swap_address = bytes_to_hex_string(&CONTRACT_SWAP);
                hmemu::call_contract(&sender1, vec![swap_address.as_str(), "100"], || {
                    assert_eq!(0, erc20::approve());
                    Ok(())
                })?;
            }
            { // open a swap contract. (sender1 is opener)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                let open_address = bytes_to_hex_string(&CONTRACT_TOKEN_OPEN);
                let close_address = bytes_to_hex_string(&CONTRACT_TOKEN_CLOSE);
                let args = vec![
                    "swap1",                // swap_id
                    "100",                  // open_value
                    open_address.as_str(),  // open_contract
                    "1",                    // close_value(tokenID)
                    SENDER2_ADDR,           // close_trader
                    close_address.as_str(), // close_contract
                ];
                hmemu::call_contract(&sender1, args, || {
                    assert_eq!(0, open_swap());
                    Ok(())
                })?;
            }
            { // approve a token on close-contract
                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;
                let swap_address = bytes_to_hex_string(&CONTRACT_SWAP);
                hmemu::call_contract(&sender2, vec![swap_address.as_str(), "1"], || {
                    assert_eq!(0, erc721::approve());
                    Ok(())
                })?;
            }
            { // close swap contract. (sender2 is closer)
                hmemu::init_contract_address(&CONTRACT_SWAP)?;
                hmemu::call_contract(&sender2, vec!["swap1"], || {
                    assert_eq!(0, close_swap());
                    Ok(())
                })?;
            }
            { // check if each balance is valid
                hmemu::init_contract_address(&CONTRACT_TOKEN_OPEN)?;                
                hmemu::call_contract(&sender1, Vec::<String>::new(), || {
                    assert_eq!(0, erc20::balanceOf());
                    let balance = hmemu::get_return_value()?;
                    assert_eq!(100000 * 10 - 100, bytes_to_i64(&balance));
                    Ok(())
                })?;
                hmemu::call_contract(&sender2, Vec::<String>::new(), || {
                    assert_eq!(0, erc20::balanceOf());
                    let balance = hmemu::get_return_value()?;
                    assert_eq!(100, bytes_to_i64(&balance));
                    Ok(())
                })?;

                hmemu::init_contract_address(&CONTRACT_TOKEN_CLOSE)?;                
                hmemu::call_contract(&sender1, vec!["1"], || {
                    assert_eq!(0, erc721::ownerOf());
                    let owner = hmemu::get_return_value()?;
                    assert_eq!(sender1, owner);
                    Ok(())
                })?;
            }

            Ok(())
        })
        .unwrap();
    }

    fn bytes_to_i64(bs: &[u8]) -> i64 {
        let mut v: [u8; 8] = Default::default();
        v.copy_from_slice(bs);

        i64::from_be_bytes(v)
    }
}
