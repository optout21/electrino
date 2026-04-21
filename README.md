# Electrino -- Block filter-based Personal Electrum server

Electrino is a bitcoin compact block filter (CBF) based Electrum server for personal use.


__UPDATE:__ This approach is not viable.
It turned out that this approach is not possible, Electrum server interface and compact block filters are not compatible.
The Electrum server receives addresses as script _hashes_, thus the script/address is not available (only after the results have been found). Compact block filters can be queried only with the script, and not the script hash.

Therefore I cease the development of this prototype.

__end of Update__


## The Vision

A fully validating, low-resource "node" to be used for any personal wallet that supports Electrum servers, designed for mostly-on setup.


## Details

__Fully validating__: Electrino does not rely on any third part for validation info, but, using compact block filters (BIP157 CBFs), it validates blockchain information (block headers, filter headers, blocks, transactions).

__P2P/Node__: While "node" is not the most descriptive word for Electrino, it is a P2P light node conneting to the bitcoin network.

__Electrum server__: Electrino offers an Electrum-compatible interface, suitable to obtain address-centric information.

__Personal__: Electrino is designed for personal use, for using with personal wallets, with a relatively low number of addresses. With the default limit of 100K addresses, there is plenty of room for large wallets, and even some larger scope usage.

__Low resource__: This means significantly less resources for IBD compared to a full bitcoin node. While a full bitcoin node requires a few days for IBD, and 900 GB for storing the chain, Electrino requires much less -- ADD EXACT NUMBERS

__Mostly-on__: Electrino works best in an "always-on" or "mostly-on" setup, like an always-on laptop, a mini PC, or a VPS. While it can work as part of a wallet with on-demand usage, that's not an optimal setup.

__Private__: Like other block filter based solutions, it is private, as the set of information requested from the network does not leak information about the own addresses.

__Stateful__: Electrino caches the block filters (about XXX GB of data), and stores the set of watched addresses persistently.


Electrino is based on `kyoto-store` -- the `kyoto` BIP157 Rust library modified for caching.


## Status

Prototype.
Works: scans the chain, caches filters, looks for addresses, derives UTXO set.
Missing: Rescans. Electrum interface.


## FAQ

Compatibility

What about comparison to a pruned bitcoin node? That does not support electrum indexers, unfortunately.


