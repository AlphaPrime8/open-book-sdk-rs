
use solana_sdk::{
    pubkey::Pubkey,
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    account::Account,
    program_error::ProgramError,
    account_info::AccountInfo,
    system_instruction,
};

use std::slice::Iter;
use std::io::Error;

#[derive(Debug)]
struct Slab {
    header: Header,
    nodes: Vec<SlabNode>,
}

#[derive(Debug)]
struct Header {
    bump_index: u32,
    free_list_len: u32,
    free_list_head: u32,
    root: u32,
    leaf_count: u32,
}

#[derive(Debug)]
enum SlabNode {
    Uninitialized,
    InnerNode {
        prefix_len: u32,
        key: u128,
        children: [u32; 2],
    },
    LeafNode {
        owner_slot: u8,
        fee_tier: u8,
        key: u128,
        owner: PublicKey,
        quantity: u64,
        client_order_id: u64,
    },
    FreeNode {
        next: u32,
    },
    LastFreeNode
}

let slab_header_layout = struct![
    u32, 
    zeros(4),
    u32, 
    zeros(4),
    u32, 
    u32, 
    u32,
];

let slab_node_layout = union![
    u32, 
    blob(68), 
    |tag| match tag {
        0 => struct![],
        1 => struct![
            u32; 
            u128; 
            seq(u32, 2); 
        ],
        2 => struct![
            u8, 
            u8, 
            blob(2),
            u128, 
            public_key_layout,
            u64, 
            u64, 
        ],
        3 => struct![
            u32; 
        ],
        4 => struct![],
    }
];

let slab_layout = struct![
    slab_header_layout,
    seq(slab_node_layout, offset(|v| v.0.bumpIndex, |v| v.0.bumpIndex - v.0.layout_size()), "nodes")
];

impl Slab {
    fn get(&self, search_key: &u128) -> Option<&SlabNode> {
        if self.header.leaf_count == 0 {
            return None;
        }
        
        let mut search_key_bn = search_key.clone();
        if let SlabNode::LeafNode { key, .. } = &self.nodes[self.header.root] {
            if let Some(leaf_node) = key.as_ref() == Some(&search_key_bn) {
                return Some(&leaf_node);
            } else {
                return None;
            }
        }
        
        let mut index = self.header.root;
        loop {
            if let SlabNode::LeafNode { key, .. } = &self.nodes[index] {
                if key.as_ref() == Some(&search_key_bn) {
                    return Some(&leaf_node);
                } else {
                    return None;
                }
            } else if let SlabNode::InnerNode { prefix_len, key, children } = &self.nodes[index] {
                if !key.bitwise_xor(&search_key_bn).shift_right(128 - prefix_len).is_zero() {
                    return None;
                }
                index = children[if search_key.testn(128 - prefix_len - 1) {1} else {0}];
            } else {
                panic!("Invalid slab");
            }
        }
    }

    pub fn iter(&self) -> Iter<'_, SlabNode> {
        self.items(false)
    }

    fn items(&self, descending: bool) -> impl Iterator<Item = &SlabNode> {
        let stack: Vec<u32> = vec![self.header.root];
        let nodes = &self.nodes;

        std::iter::from_fn(move || {
            if let Some(index) = stack.last().copied() {
                let node = &nodes[index as usize];
                stack.pop();

                if let SlabNode::LeafNode { .. } = node {
                    return Some(node);
                } else if let SlabNode::InnerNode { children, .. } = node {
                    if descending {
                        stack.push(children[0]);
                        stack.push(children[1]);
                    } else {
                        stack.push(children[1]);
                        stack.push(children[0]);
                    }
                }
            }

            None
        }).filter_map(std::convert::identity)
    }
}

export function setLayoutDecoder(layout, decoder) {
    const originalDecode = layout.decode;
    layout.decode = function decode(b, offset = 0) {
      return decoder(originalDecode.call(this, b, offset));
    };
}
  
export function setLayoutEncoder(layout, encoder) {
    const originalEncode = layout.encode;
    layout.encode = function encode(src, b, offset) {
        return originalEncode.call(this, encoder(src), b, offset);
    };
    return layout;
}

setLayoutDecoder(SLAB_LAYOUT, |data| {
    let header = Header {
        bump_index: data[0],
        free_list_len: data[2],
        free_list_head: data[5],
        root: data[6],
        leaf_count: data[8],
    };

    Slab { header, nodes }
});