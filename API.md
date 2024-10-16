# RPC API

## Difference

### Triton vs Original:

#### `getLatestBlockhash`

Accept `rollback` option that send slightly outdated blockhash, can be useful if transaction send through RPC node that behind network.

#### `getRecentPrioritizationFees`

Accept `percentile` option that calculate fee as percentile (bps, i.e. allowed value is `[0; 10_000]`) from all matched transactions.

### Solfees vs Triton

#### `getRecentPrioritizationFees`

Provide more filters: accept read-write accounts, read-only accounts and up to 5 desired percentile levels. Response includes number of transactions, number of filtered transactions, average fee and requested levels with slot and commitment.

## Original Solana API

Endpoint: `https://api.solfees.io/api/solana`

Supported methods (same reuest and response as original Solana RPC API):

### `getLatestBlockhash`

defaults:

  - `commitment`: `finalized`
  - `min_context_slot`: `null`

```
> {"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"commitment": "confirmed", "min_context_slot": null}],"id":"1"}
< {"jsonrpc":"2.0","result":{"context":{"apiVersion":"2.0.8","slot":291299118},"value":{"blockhash":"7uEAgwnqXrA7VEjDuRninCbKkAeJNYY1zMMorvRMdZnH","lastValidBlockHeight":270196625}},"id":"1"}
```

with `cURL`:

```
curl https://api.solfees.io/api/solana -X POST -H "Content-Type: application/json" -d '{"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"commitment": "confirmed", "min_context_slot": null}],"id":"1"}'
```

### `getRecentPrioritizationFees`

defaults:

  - no / empty array

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[["S6qY45yeSJrbGB4v6ioSCj3RfLZ8JVEPdU876vWWvCq"]],"id":"1"}
< {"jsonrpc":"2.0","result":[{"prioritizationFee":0,"slot":291299263},{"prioritizationFee":0,"slot":291299264},{"prioritizationFee":0,"slot":291299265},...,{"prioritizationFee":0,"slot":291299413}],"id":"1"}
```

### `getSlot`

defaults:

  - `min_context_slot`: `null`

```
> {"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"commitment": "confirmed", "min_context_slot": null}],"id":"1"}
< {"jsonrpc":"2.0","result":{"context":{"apiVersion":"2.0.8","slot":291299118},"value":{"blockhash":"7uEAgwnqXrA7VEjDuRninCbKkAeJNYY1zMMorvRMdZnH","lastValidBlockHeight":270196625}},"id":"1"}
```

### `getVersion`

```
> {"method":"getVersion","jsonrpc":"2.0","params":[],"id":"1"}
< {"jsonrpc":"2.0","result":{"feature-set":1420694968,"solana-core":"2.0.8"},"id":"1"}
```

## Extended Solana API (patches made by Triton)

Endpoint: `https://api.solfees.io/api/solana/triton`

Supported methods (backward compatiable with original Solana RPC API, i.e. you can send same request and you will receive same response response structure):

### `getLatestBlockhash`

defaults:

  - `rollback`: `0`
  - `commitment`: `finalized`
  - `min_context_slot`: `null`

```
> {"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"rollback": 42, "commitment": "finalized", "min_context_slot": null}],"id":"1"}
< {"jsonrpc":"2.0","result":{"context":{"apiVersion":"2.0.8","slot":291299118},"value":{"blockhash":"7uEAgwnqXrA7VEjDuRninCbKkAeJNYY1zMMorvRMdZnH","lastValidBlockHeight":270196625}},"id":"1"}
```

### `getRecentPrioritizationFees`

defaults:

  - no / empty array
  - `percentile`: `0`

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[["S6qY45yeSJrbGB4v6ioSCj3RfLZ8JVEPdU876vWWvCq"]],"id":"1"}
< {"jsonrpc":"2.0","result":[{"prioritizationFee":0,"slot":291299263},{"prioritizationFee":0,"slot":291299264},{"prioritizationFee":0,"slot":291299265},...,{"prioritizationFee":0,"slot":291299413}],"id":"1"}
```

### `getSlot`

No changes compare to Solana API.

### `getVersion`

No changes compare to Solana API.

## Solfees Solana API

Endpoint: `https://api.solfees.io/api/solana/solfees`

Supported methods:

### `getLatestBlockhash`

No changes compare to Solana API patched by Triton.

### `getRecentPrioritizationFees`

Total number of `readWrite` + `readOnly` accounts should be less than 128. Up to 5 levels allowed.

defaults:

  - `readWrite`: `[]`
  - `readOnly`: `[]`
  - `percentile`: `0`

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[{"readWrite":[],"readOnly":["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"],"levels":[5000, 9500]}],"id":"1"}
< {"jsonrpc":"2.0","result":[{"commitment":"finalized","feeAverage":86553.02597402598,"feeLevels":[7000,417947],"height":270953519,"slot":292082694,"totalTransactions":919,"totalTransactionsFiltered":154,"totalTransactionsVote":707},...,{"commitment":"confirmed","feeAverage":23633.71891891892,"feeLevels":[8089,61000],"height":270953670,"slot":292082846,"totalTransactions":988,"totalTransactionsFiltered":185,"totalTransactionsVote":746}],"id":"1"}
```

More control over read-only / read-write accounts, total number of transactions, average fee and up to 5 levels (as percentile).

### `getSlot`

No changes compare to Solana API.

### `getVersion`

No changes compare to Solana API.

# WebSocket API

Endpoint: `https://api.solfees.io/api/solana/solfees/ws`

Total number of `readWrite` + `readOnly` accounts should be less than 128. Up to 5 levels allowed.

```
> {"id":0,"method":"SlotsSubscribe","params":{"readWrite":[],"readOnly":["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"],"levels":[5000,9500]}}
< {"result":"subscribed","id":0}
< {"result":{"slot":{"commitment":"processed","feeAverage":44220.26190476191,"feeLevels":[],"hash":"FdN8HGjbzap3EHeRtRAdQyTDjPM4k4DPEsuUuv91coiD","height":270978968,"identity":"11111111111111111111111111111111","slot":292109054,"time":1727350992,"totalFee":12156255,"totalTransactions":784,"totalTransactionsFiltered":210,"totalTransactionsVote":574,"totalUnitsConsumed":44060484}},"id":0}
< {"result":{"status":{"commitment":"processed","slot":292109054}},"id":0}
< {"result":{"status":{"commitment":"finalized","slot":292109023}},"id":0}
< {"result":{"status":{"commitment":"confirmed","slot":292109053}},"id":0}
< {"result":{"status":{"commitment":"confirmed","slot":292109054}},"id":0}
```

With `solfees-ws-client` tool from the repo:

```
$ cargo run --bin solfees-ws-client -- --read-only TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
   Compiling solfees-be v1.0.0 (/home/kirill/projects/solfees-public/solfees-be)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.14s
     Running `target/debug/solfees-ws-client --read-only TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`
2024-10-16T10:49:40.876707Z  INFO solfees_ws_client: new message: {"result":"subscribed","id":0}
2024-10-16T10:49:41.185825Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"confirmed","slot":295932428}},"id":0}
2024-10-16T10:49:41.185882Z  INFO solfees_ws_client: new message: {"result":{"slot":{"commitment":"processed","feeAverage":79729.71717171717,"feeLevels":[0,0,356241],"height":274652614,"slot":295932429,"totalTransactions":1586,"totalTransactionsFiltered":99,"totalTransactionsVote":1284}},"id":0}
2024-10-16T10:49:41.185923Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"processed","slot":295932429}},"id":0}
2024-10-16T10:49:41.185941Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"finalized","slot":295932398}},"id":0}
2024-10-16T10:49:41.428988Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"confirmed","slot":295932429}},"id":0}
2024-10-16T10:49:41.695999Z  INFO solfees_ws_client: new message: {"result":{"slot":{"commitment":"processed","feeAverage":792214.4206896551,"feeLevels":[0,0,356241],"height":274652615,"slot":295932430,"totalTransactions":1505,"totalTransactionsFiltered":145,"totalTransactionsVote":1236}},"id":0}
2024-10-16T10:49:41.696041Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"processed","slot":295932430}},"id":0}
2024-10-16T10:49:41.696058Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"finalized","slot":295932399}},"id":0}
2024-10-16T10:49:41.971094Z  INFO solfees_ws_client: new message: {"result":{"status":{"commitment":"confirmed","slot":295932430}},"id":0}
2024-10-16T10:49:42.314014Z  INFO solfees_ws_client: new message: {"result":{"slot":{"commitment":"processed","feeAverage":5634600.90821256,"feeLevels":[0,200000,8431933],"height":274652616,"slot":295932431,"totalTransactions":1195,"totalTransactionsFiltered":207,"totalTransactionsVote":823}},"id":0}
```
