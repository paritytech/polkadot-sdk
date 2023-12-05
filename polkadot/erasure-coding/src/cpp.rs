use super::*;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Shard {
	pub len: usize,
	pub data: *mut u8,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Shards {
	pub original_ptr: *mut ::std::os::raw::c_void,
	pub count: usize,
	pub shards: *mut Shard,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DecodedData {
    pub original_ptr: *mut ::std::os::raw::c_void,
    pub len: usize,
    pub data: *mut u8,
}

#[link(name = "reed-solomon-c", kind = "static")]
extern "C" {
	#[link_name = "\u{1}__Z6encodemPhmP6Shards"]
	pub fn encode(n_validators: usize, bytes: *mut u8, bytes_len: usize, output: *mut Shards)
		-> u8;

    #[link_name = "\u{1}__Z11drop_shardsPv"]
    pub fn drop_shards(original_ptr: *mut ::std::os::raw::c_void);

	#[link_name = "\u{1}__Z6decodemP5Shardm"]
    pub fn decode(n_validators: usize, shards: *mut Shard, n_shards: usize) -> DecodedData;

    #[link_name = "\u{1}__Z17drop_decoded_dataPv"]
    pub fn drop_decoded_data(original_ptr: *mut ::std::os::raw::c_void);
}

pub fn obtain_chunks(n_validators: usize, data: &AvailableData) -> Vec<Vec<u8>> {
	let mut encoded = data.encode();
	assert!(!encoded.is_empty());

	let mut shard_vec: Vec<Shard> =
		(0..n_validators).map(|_| Shard { len: 0, data: std::ptr::null_mut() }).collect();
	let mut shards =
		Shards { original_ptr: std::ptr::null_mut(), count: n_validators, shards: shard_vec.as_mut_ptr() };
	let len = encoded.len();
	let res =
		unsafe { encode(n_validators, encoded.as_mut_ptr(), len, &mut shards as *mut Shards) };
	assert_eq!(res, 0);
	shard_vec.leak();

	assert_eq!(shards.count, n_validators);

    let original_ptr = shards.original_ptr;
    let slice = unsafe { std::slice::from_raw_parts(shards.shards, shards.count) };
	let res = slice
		.iter()
		.map(|chunk| unsafe {std::slice::from_raw_parts(chunk.data, chunk.len)}.to_vec());

	let res = res.collect();

    unsafe {
        drop_shards(original_ptr);
    }

    res
}

pub fn reconstruct(n_validators: usize, chunks: Vec<&[u8]>) -> AvailableData
{  
    let n_chunks = chunks.len();
    let mut shards: Vec<_> = chunks.into_iter().map(|chunk| {
        Shard {
            data: chunk.as_ptr() as *mut _,
            len: chunk.len()
        }
    }).collect();
    let decoded_data = unsafe {decode(n_validators, shards.as_mut_ptr(), n_chunks)};
    let bytes = unsafe {std::slice::from_raw_parts(decoded_data.data, decoded_data.len)}.to_vec();

	let res = Decode::decode(&mut &bytes[..]).unwrap();

    unsafe {
        drop_decoded_data(decoded_data.original_ptr);
    }

    res
}

