// Miniscript
// Written in 2018 by
//     Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! Lexer
//!
//! Translates a script into a reversed sequence of tokens
//!

use bitcoin::PublicKey;
use elements::{opcodes, script};

use std::fmt;

use super::Error;
use util::{build_scriptint, slice_to_u32_le};
/// Atom of a tokenized version of a script
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Token {
    BoolAnd,
    BoolOr,
    Add,
    Sub,
    Equal,
    CheckSig,
    CheckSigFromStack,
    CheckSigFromStackVerify,
    CheckMultiSig,
    CheckSequenceVerify,
    CheckLockTimeVerify,
    FromAltStack,
    ToAltStack,
    Left,
    Cat,
    CodeSep,
    Over,
    Pick,
    Depth,
    Drop,
    Dup,
    If,
    IfDup,
    NotIf,
    Else,
    EndIf,
    ZeroNotEqual,
    Size,
    Swap,
    Verify,
    Ripemd160,
    Hash160,
    Sha256,
    Hash256,
    Num(u32),
    Hash20([u8; 20]),
    Hash32([u8; 32]),
    Pubkey(PublicKey),
    Push(Vec<u8>),        // Num or a
    PickPush4(u32),       // Pick followed by a 4 byte push
    PickPush32([u8; 32]), // Pick followed by a 32 byte push
    PickPush(Vec<u8>),    // Pick followed by a push
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Num(n) => write!(f, "#{}", n),
            Token::Hash20(hash) => {
                for ch in &hash[..] {
                    write!(f, "{:02x}", *ch)?;
                }
                Ok(())
            }
            Token::Hash32(hash) => {
                for ch in &hash[..] {
                    write!(f, "{:02x}", *ch)?;
                }
                Ok(())
            }
            Token::Pubkey(pk) => write!(f, "{}", pk),
            x => write!(f, "{:?}", x),
        }
    }
}

#[derive(Debug, Clone)]
/// Iterator that goes through a vector of tokens backward (our parser wants to read
/// backward and this is more efficient anyway since we can use `Vec::pop()`).
// This really does not need to be an iterator because the way we are using it, we are
// actually collecting lexed symbols into a vector. If that is the case, might as well
// use the inner vector directly
pub struct TokenIter(Vec<Token>);

impl TokenIter {
    /// Create a new TokenIter
    pub fn new(v: Vec<Token>) -> TokenIter {
        TokenIter(v)
    }

    /// Look at the top at Iterator
    pub fn peek(&self) -> Option<&Token> {
        self.0.last()
    }

    /// Look at the slice with the last n elements
    pub fn peek_slice(&self, n: usize) -> Option<&[Token]> {
        if n <= self.len() {
            Some(self.0[self.len() - n..].as_ref())
        } else {
            None
        }
    }

    /// Advance the iterator n times
    /// Returns Some(()) if the iterator can be advanced n times
    pub fn advance(&mut self, n: usize) -> Option<()> {
        if n <= self.len() {
            for _ in 0..n {
                self.next();
            }
            Some(())
        } else {
            None
        }
    }

    /// Push a value to the iterator
    /// This will be first value consumed by popun_
    pub fn un_next(&mut self, tok: Token) {
        self.0.push(tok)
    }

    /// The len of the iterator
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Iterator for TokenIter {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.0.pop()
    }
}

/// Tokenize a script
pub fn lex(script: &script::Script) -> Result<Vec<Token>, Error> {
    let mut ret = Vec::with_capacity(script.len());

    fn process_candidate_push(ret: &mut Vec<Token>) -> Result<(), Error> {
        let ret_len = ret.len();

        if ret_len < 2 || ret[ret_len - 1] != Token::Swap {
            return Ok(());
        }
        let token = match &ret[ret_len - 2] {
            Token::Hash20(x) => Token::Push(x.to_vec()),
            Token::Hash32(x) => Token::Push(x.to_vec()),
            Token::Pubkey(pk) => Token::Push(pk.to_bytes()),
            Token::Num(k) => Token::Push(build_scriptint(*k as i64)),
            _x => return Ok(()), // no change required
        };
        ret[ret_len - 2] = token;
        Ok(())
    }

    for ins in script.instructions_minimal() {
        match ins.map_err(Error::Script)? {
            script::Instruction::Op(opcodes::all::OP_BOOLAND) => {
                ret.push(Token::BoolAnd);
            }
            script::Instruction::Op(opcodes::all::OP_BOOLOR) => {
                ret.push(Token::BoolOr);
            }
            script::Instruction::Op(opcodes::all::OP_EQUAL) => {
                ret.push(Token::Equal);
            }
            script::Instruction::Op(opcodes::all::OP_EQUALVERIFY) => {
                ret.push(Token::Equal);
                ret.push(Token::Verify);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKSIG) => {
                ret.push(Token::CheckSig);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKSIGFROMSTACK) => {
                ret.push(Token::CheckSigFromStack);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKSIGFROMSTACKVERIFY) => {
                ret.push(Token::CheckSigFromStackVerify);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKSIGVERIFY) => {
                ret.push(Token::CheckSig);
                ret.push(Token::Verify);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKMULTISIG) => {
                ret.push(Token::CheckMultiSig);
            }
            script::Instruction::Op(opcodes::all::OP_CHECKMULTISIGVERIFY) => {
                ret.push(Token::CheckMultiSig);
                ret.push(Token::Verify);
            }
            script::Instruction::Op(op) if op == opcodes::all::OP_CSV => {
                ret.push(Token::CheckSequenceVerify);
            }
            script::Instruction::Op(op) if op == opcodes::all::OP_CLTV => {
                ret.push(Token::CheckLockTimeVerify);
            }
            script::Instruction::Op(opcodes::all::OP_FROMALTSTACK) => {
                ret.push(Token::FromAltStack);
            }
            script::Instruction::Op(opcodes::all::OP_TOALTSTACK) => {
                ret.push(Token::ToAltStack);
            }
            script::Instruction::Op(opcodes::all::OP_LEFT) => {
                ret.push(Token::Left);
            }
            script::Instruction::Op(opcodes::all::OP_CAT) => {
                process_candidate_push(&mut ret)?;
                ret.push(Token::Cat);
            }
            script::Instruction::Op(opcodes::all::OP_CODESEPARATOR) => {
                ret.push(Token::CodeSep);
            }
            script::Instruction::Op(opcodes::all::OP_OVER) => {
                ret.push(Token::Over);
            }
            script::Instruction::Op(opcodes::all::OP_PICK) => {
                ret.push(Token::Pick);
            }
            script::Instruction::Op(opcodes::all::OP_DROP) => {
                ret.push(Token::Drop);
            }
            script::Instruction::Op(opcodes::all::OP_DEPTH) => {
                ret.push(Token::Depth);
            }
            script::Instruction::Op(opcodes::all::OP_DUP) => {
                ret.push(Token::Dup);
            }
            script::Instruction::Op(opcodes::all::OP_ADD) => {
                ret.push(Token::Add);
            }
            script::Instruction::Op(opcodes::all::OP_SUB) => {
                ret.push(Token::Sub);
            }
            script::Instruction::Op(opcodes::all::OP_IF) => {
                ret.push(Token::If);
            }
            script::Instruction::Op(opcodes::all::OP_IFDUP) => {
                ret.push(Token::IfDup);
            }
            script::Instruction::Op(opcodes::all::OP_NOTIF) => {
                ret.push(Token::NotIf);
            }
            script::Instruction::Op(opcodes::all::OP_ELSE) => {
                ret.push(Token::Else);
            }
            script::Instruction::Op(opcodes::all::OP_ENDIF) => {
                ret.push(Token::EndIf);
            }
            script::Instruction::Op(opcodes::all::OP_0NOTEQUAL) => {
                ret.push(Token::ZeroNotEqual);
            }
            script::Instruction::Op(opcodes::all::OP_SIZE) => {
                ret.push(Token::Size);
            }
            script::Instruction::Op(opcodes::all::OP_SWAP) => {
                ret.push(Token::Swap);
            }
            script::Instruction::Op(opcodes::all::OP_VERIFY) => {
                match ret.last() {
                    Some(op @ &Token::Equal)
                    | Some(op @ &Token::CheckSig)
                    | Some(op @ &Token::CheckMultiSig) => {
                        return Err(Error::NonMinimalVerify(op.clone()))
                    }
                    _ => {}
                }
                ret.push(Token::Verify);
            }
            script::Instruction::Op(opcodes::all::OP_RIPEMD160) => {
                ret.push(Token::Ripemd160);
            }
            script::Instruction::Op(opcodes::all::OP_HASH160) => {
                ret.push(Token::Hash160);
            }
            script::Instruction::Op(opcodes::all::OP_SHA256) => {
                ret.push(Token::Sha256);
            }
            script::Instruction::Op(opcodes::all::OP_HASH256) => {
                ret.push(Token::Hash256);
            }
            script::Instruction::PushBytes(bytes) => {
                // Check for Pick Push
                // Special handling of tokens for Covenants
                // To determine whether some Token is actually
                // 4 bytes push or a script int of 4 bytes,
                // we need additional script context
                if ret.last() == Some(&Token::Pick) {
                    ret.pop().unwrap();
                    match bytes.len() {
                        // All other sighash elements are 32 bytes. And the script code
                        // is 24 bytes
                        4 => ret.push(Token::PickPush4(slice_to_u32_le(bytes))),
                        32 => {
                            let mut x = [0u8; 32];
                            x.copy_from_slice(bytes);
                            ret.push(Token::PickPush32(x));
                        }
                        // Other pushes should be err. This will change
                        // once we add script introspection
                        _ => return Err(Error::InvalidPush(bytes.to_owned())),
                    }
                } else {
                    // Create the most specific type possible out of the
                    // Push. When we later encounter CAT, revisit and
                    // reconvert these to pushes.
                    // See [process_candidate_push]
                    match bytes.len() {
                        20 => {
                            let mut x = [0; 20];
                            x.copy_from_slice(bytes);
                            ret.push(Token::Hash20(x));
                        }
                        32 => {
                            let mut x = [0; 32];
                            x.copy_from_slice(bytes);
                            ret.push(Token::Hash32(x));
                        }
                        33 | 65 => {
                            ret.push(Token::Pubkey(
                                PublicKey::from_slice(bytes).map_err(Error::BadPubkey)?,
                            ));
                        }
                        _ => {
                            match script::read_scriptint(bytes) {
                                Ok(v) if v >= 0 => {
                                    // check minimality of the number
                                    if &script::Builder::new().push_int(v).into_script()[1..]
                                        != bytes
                                    {
                                        return Err(Error::InvalidPush(bytes.to_owned()));
                                    }
                                    ret.push(Token::Num(v as u32));
                                }
                                _ => ret.push(Token::Push(bytes.to_owned())),
                            }
                        }
                    }
                }
            }
            script::Instruction::Op(opcodes::all::OP_PUSHBYTES_0) => {
                ret.push(Token::Num(0));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_1) => {
                ret.push(Token::Num(1));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_2) => {
                ret.push(Token::Num(2));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_3) => {
                ret.push(Token::Num(3));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_4) => {
                ret.push(Token::Num(4));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_5) => {
                ret.push(Token::Num(5));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_6) => {
                ret.push(Token::Num(6));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_7) => {
                ret.push(Token::Num(7));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_8) => {
                ret.push(Token::Num(8));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_9) => {
                ret.push(Token::Num(9));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_10) => {
                ret.push(Token::Num(10));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_11) => {
                ret.push(Token::Num(11));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_12) => {
                ret.push(Token::Num(12));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_13) => {
                ret.push(Token::Num(13));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_14) => {
                ret.push(Token::Num(14));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_15) => {
                ret.push(Token::Num(15));
            }
            script::Instruction::Op(opcodes::all::OP_PUSHNUM_16) => {
                ret.push(Token::Num(16));
            }
            script::Instruction::Op(op) => return Err(Error::InvalidOpcode(op)),
        };
    }
    Ok(ret)
}
