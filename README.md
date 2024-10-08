# solfees.io

## RPC API

### Difference

#### Triton vs Original:

##### `getLatestBlockhash`

Accept `rollback` option that send slightly outdated blockhash, can be useful if transaction send through RPC node that behind network.

##### `getRecentPrioritizationFees`

Accept `percentile` option that calculated fee as percentile (bps, i.e. allowed value is `[0; 10_000]`) from all matched transactions.

#### Solfees vs Triton

##### `getRecentPrioritizationFees`

Provide more filters: accept read-write accounts, read-only accounts and up to 5 desired percentile levels. Response includes number of transactions, number of filtered transactions, average fee and requested levels with slot and commitment.

### Original Solana API

Endpoint: `https://api.solfees.io/api/solana`

Supported methods (same reuest and response as original Solana RPC API):

#### `getLatestBlockhash`

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

#### `getRecentPrioritizationFees`

defaults:

  - no / empty array

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[["S6qY45yeSJrbGB4v6ioSCj3RfLZ8JVEPdU876vWWvCq"]],"id":"1"}
< {"jsonrpc":"2.0","result":[{"prioritizationFee":0,"slot":291299263},{"prioritizationFee":0,"slot":291299264},{"prioritizationFee":0,"slot":291299265},...,{"prioritizationFee":0,"slot":291299413}],"id":"1"}
```

#### `getSlot`

defaults:

  - `min_context_slot`: `null`

```
> {"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"commitment": "confirmed", "min_context_slot": null}],"id":"1"}
< {"jsonrpc":"2.0","result":{"context":{"apiVersion":"2.0.8","slot":291299118},"value":{"blockhash":"7uEAgwnqXrA7VEjDuRninCbKkAeJNYY1zMMorvRMdZnH","lastValidBlockHeight":270196625}},"id":"1"}
```

#### `getVersion`

```
> {"method":"getVersion","jsonrpc":"2.0","params":[],"id":"1"}
< {"jsonrpc":"2.0","result":{"feature-set":1420694968,"solana-core":"2.0.8"},"id":"1"}
```

### Extended Solana API (patches made by Triton)

Endpoint: `https://api.solfees.io/api/solana/triton`

Supported methods (backward compatiable with original Solana RPC API, i.e. you can send same request and you will receive same response response structure):

#### `getLatestBlockhash`

defaults:

  - `rollback`: `0`
  - `commitment`: `finalized`
  - `min_context_slot`: `null`

```
> {"method":"getLatestBlockhash","jsonrpc":"2.0","params":[{"rollback": 42, "commitment": "finalized", "min_context_slot": null}],"id":"1"}
< {"jsonrpc":"2.0","result":{"context":{"apiVersion":"2.0.8","slot":291299118},"value":{"blockhash":"7uEAgwnqXrA7VEjDuRninCbKkAeJNYY1zMMorvRMdZnH","lastValidBlockHeight":270196625}},"id":"1"}
```

#### `getRecentPrioritizationFees`

defaults:

  - no / empty array
  - `percentile`: `0`

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[["S6qY45yeSJrbGB4v6ioSCj3RfLZ8JVEPdU876vWWvCq"]],"id":"1"}
< {"jsonrpc":"2.0","result":[{"prioritizationFee":0,"slot":291299263},{"prioritizationFee":0,"slot":291299264},{"prioritizationFee":0,"slot":291299265},...,{"prioritizationFee":0,"slot":291299413}],"id":"1"}
```

#### `getSlot`

No changes compare to Solana API.

#### `getVersion`

No changes compare to Solana API.

### Solfees Solana API

Endpoint: `https://api.solfees.io/api/solana/solfees`

Supported methods:

#### `getLatestBlockhash`

No changes compare to Solana API patched by Triton.

#### `getRecentPrioritizationFees`

defaults:

  - `readWrite`: `[]`
  - `readOnly`: `[]`
  - `percentile`: `0`

```
> {"method":"getRecentPrioritizationFees","jsonrpc":"2.0","params":[{"readWrite":[],"readOnly":["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"],"levels":[5000, 9500]}],"id":"1"}
< {"jsonrpc":"2.0","result":[{"commitment":"finalized","feeAverage":86553.02597402598,"feeLevels":[7000,417947],"height":270953519,"slot":292082694,"totalTransactions":919,"totalTransactionsFiltered":154,"totalTransactionsVote":707},...,{"commitment":"confirmed","feeAverage":23633.71891891892,"feeLevels":[8089,61000],"height":270953670,"slot":292082846,"totalTransactions":988,"totalTransactionsFiltered":185,"totalTransactionsVote":746}],"id":"1"}
```

More control over read-only / read-write accounts, total number of transactions, average fee and up to 5 levels (as percentile).

#### `getSlot`

No changes compare to Solana API.

#### `getVersion`

No changes compare to Solana API.

## WebSocket API

Endpoint: `https://api.solfees.io/api/solana/solfees/ws`

```
> {"id":0,"method":"SlotsSubscribe","params":{"readWrite":[],"readOnly":["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"],"levels":[5000,9500]}}
< {"result":"subscribed","id":0}
< {"result":{"slot":{"commitment":"processed","feeAverage":44220.26190476191,"feeLevels":[],"hash":"FdN8HGjbzap3EHeRtRAdQyTDjPM4k4DPEsuUuv91coiD","height":270978968,"identity":"11111111111111111111111111111111","slot":292109054,"time":1727350992,"totalFee":12156255,"totalTransactions":784,"totalTransactionsFiltered":210,"totalTransactionsVote":574,"totalUnitsConsumed":44060484}},"id":0}
< {"result":{"status":{"commitment":"processed","slot":292109054}},"id":0}
< {"result":{"status":{"commitment":"finalized","slot":292109023}},"id":0}
< {"result":{"status":{"commitment":"confirmed","slot":292109053}},"id":0}
< {"result":{"status":{"commitment":"confirmed","slot":292109054}},"id":0}
```
