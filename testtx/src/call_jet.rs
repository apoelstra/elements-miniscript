
use std::sync::Arc;

use elements_miniscript::elements;

use simplicity::ffi::CFrameItem; 
use simplicity::jet::Elements; 
use simplicity::jet::elements::ElementsEnv; 

const MAX_VALUE_BYTES: usize = 256; // whatever don't worry

/// Internal helper function that just calls a jet on a flatvalue to compute
/// a new flatvalue. We don't use this for benchmarks because it doesn't mess
/// with alignment and isn't very careful about type correctness/sizing.
pub fn call_jet(env: &ElementsEnv<Arc<elements::Transaction>>, jet: Elements, input: &[u8]) -> (Vec<u8>, usize)
{
    use core::{mem, ptr};
    use simplicity::ffi::c_jets::uword_width;
    use simplicity::jet::Jet as _;

    let src_n_bits = jet.source_ty().to_final().bit_width();
    let src_n_bytes = (src_n_bits + 7) / 8;
    let dest_n_bits = jet.target_ty().to_final().bit_width();
    let dest_n_bytes = (dest_n_bits + 7) / 8;

    assert!(input.len() >= src_n_bits * 8);
    let mut ret = Vec::with_capacity(MAX_VALUE_BYTES);
    for _ in 0..dest_n_bytes {
        ret.push(0);
    }

    unsafe {
        let mut dst_inner = [0usize; MAX_VALUE_BYTES / mem::size_of::<usize>()];
        let mut src_inner = [0usize; MAX_VALUE_BYTES / mem::size_of::<usize>()];

        let mut src_bytes = input.to_vec();
        if !src_bytes.is_empty() {
            // See below block comment on the write frame for justification of this
            // weird byte-swapping ritual.
            src_bytes[..src_n_bytes].reverse();
            ptr::copy_nonoverlapping(
                src_bytes.as_ptr(),
                src_inner.as_mut_ptr() as *mut u8,
                MAX_VALUE_BYTES,
            );
            for us in &mut src_inner {
                *us = usize::from_be(us.swap_bytes());
            }
        }

        let src_read_frame = CFrameItem::new_read(src_n_bits, src_inner.as_ptr());
        let mut dst_write_frame = CFrameItem::new_write(
            dest_n_bits,
            dst_inner.as_mut_ptr().add(uword_width(dest_n_bits)),
        );


        // We can assert this because in our sampling code jets should never
        // fail. In the benchmarking code they might.
        assert!(jet.c_jet_ptr()(
            &mut dst_write_frame,
            src_read_frame,
            Elements::c_jet_env(&env)
        ));
        // The write frame winds up as an array of usizes with all bytes in
        // reverse order. (The bytes of the usizes are in reverse order due
        // to endianness, but also the usizes themselves are in reverse
        // order for whatever reason.)If the number of bits written was not
        // a multiple of 8, then the final usize will be incomplete and its
        // **least** significant byte(s) will be 0 and of the nonzero byte
        // the **most significant bit(s)** will be 0.
        //
        // To solve this, we first convert the backward usize array to a
        // backward u8 array...
        for us in &mut dst_inner {
            *us = us.swap_bytes().to_be();
        }
        ptr::copy_nonoverlapping(
            dst_inner.as_ptr() as *mut u8,
            ret.as_mut_ptr(),
            MAX_VALUE_BYTES,
        );

        // We then reverse the backward byte array, which leaves us with a
        // correct-direction byte array which may be *right*-shifted.
        if dest_n_bits % 8 == 0 {
            ret[..dest_n_bits / 8].reverse();
        } else {
            unreachable!("haven't implemented unaligned output yet")
        }
    }
    (ret, dest_n_bits)
}
