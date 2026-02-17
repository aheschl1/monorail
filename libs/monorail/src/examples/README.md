# Monorail Runtime Examples

This directory contains example code demonstrating different features and usage patterns of the Monorail runtime.

## Running Examples

All examples are structured as test cases. Run them using:

```bash
# Run all examples
cargo test --lib examples::distributed

# Run a specific example
cargo test --lib examples::distributed::<test_name> -- --nocapture
```

## Available Examples

### 1. `test_cross_shard_computation`
**Demonstrates:** Cross-shard task submission using `call_on`

Shows how to:
- Submit tasks to specific shards
- Collect and aggregate results from multiple shards
- Distribute computational work across CPU cores

**Key APIs:**
- `monolib::call_on(shard_id, closure)`
- `monolib::shard_id()`
- `monolib::get_topology_info()`

```bash
cargo test --lib examples::distributed::test_cross_shard_computation -- --nocapture
```

### 2. `test_actor_based_processing`
**Demonstrates:** Actor-based distributed processing

Shows how to:
- Spawn coordinator and worker actors
- Distribute work through actor messages
- Register actors for discovery
- Implement complex coordination patterns

**Key APIs:**
- `monolib::spawn_actor::<ActorType>(args)`
- `actors::register(name, address)`
- Actor message passing with `actor.send(message).await`

```bash
cargo test --lib examples::distributed::test_actor_based_processing -- --nocapture
```

### 3. `test_broadcast_to_all_shards`
**Demonstrates:** Broadcasting with `call_on_all`

Shows how to:
- Send the same task to all shards simultaneously
- Collect results from all shards
- Perform parallel execution across all CPU cores

**Key APIs:**
- `monolib::call_on_all(closure)`

```bash
cargo test --lib examples::distributed::test_broadcast_to_all_shards -- --nocapture
```

### 4. `test_async_operations`
**Demonstrates:** Async operations and timing

Shows how to:
- Use async sleep with io_uring
- Run concurrent tasks across shards
- Time async operations

**Key APIs:**
- `monolib::sleep(duration)`

```bash
cargo test --lib examples::distributed::test_async_operations -- --nocapture
```

### 5. `test_submit_to_fire_and_forget`
**Demonstrates:** Fire-and-forget task submission

Shows how to:
- Submit tasks without waiting for results
- One-way communication pattern
- Asynchronous task dispatch

**Key APIs:**
- `monolib::submit_to(shard_id, closure)`

```bash
cargo test --lib examples::distributed::test_submit_to_fire_and_forget -- --nocapture
```

### 6. `test_simple_actor_lifecycle`
**Demonstrates:** Basic actor lifecycle

Shows how to:
- Create an actor with initial state
- Handle messages and mutate state
- Gracefully shutdown actors
- Implement request-response patterns

**Key APIs:**
- `Actor` trait implementation
- `pre_start`, `handle`, `post_stop` lifecycle methods
- `actor.stop()` for graceful shutdown

```bash
cargo test --lib examples::distributed::test_simple_actor_lifecycle -- --nocapture
```

### 7. `test_worker_actor_message_passing`
**Demonstrates:** Worker actor with structured messages

Shows how to:
- Process work items with return channels
- Implement request-response via oneshot channels
- Handle structured message types

**Key APIs:**
- `Actor::Message` associated type
- `oneshot::channel()` for return values

```bash
cargo test --lib examples::distributed::test_worker_actor_message_passing -- --nocapture
```

## API Quick Reference

### Core Runtime APIs

```rust
// Initialize runtime
MonorailTopology::normal(|| async {
    // Your code here
    signal_monorail(Ok(()));
})

// Query topology
let info = monolib::get_topology_info();
let cores = info.cores;
let current = monolib::shard_id();

// Cross-shard execution
monolib::call_on(shard_id, || async { /* task */ }).await?;
monolib::call_on_all(|| async { /* task */ }).await;
monolib::submit_to(shard_id, || async { /* task */ });

// Async primitives
monolib::sleep(Duration::from_millis(100)).await;
```

### Actor APIs

```rust
// Define an actor
struct MyActor;

impl Actor for MyActor {
    type Message = MyMessage;
    type Arguments = MyArgs;
    type State = MyState;
    
    async fn pre_start(args: Self::Arguments) -> Self::State {
        // Initialize state
    }
    
    async fn handle(
        this: SelfAddr<'_, Self>,
        message: Self::Message,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        // Handle message
    }
    
    async fn post_stop(
        this: SelfAddr<'_, Self>,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        // Cleanup
    }
}

// Spawn and use actors
let actor = monolib::spawn_actor::<MyActor>(args);
actors::register("my_actor", actor.clone()).await?;
actor.send(message).await?;
actor.stop();
```

## Design Patterns

### Pattern 1: Map-Reduce
Distribute work across shards, collect and aggregate results.
See: `test_cross_shard_computation`

### Pattern 2: Coordinator Pattern
Central coordinator distributes work to worker actors.
See: `test_actor_based_processing`

### Pattern 3: Broadcast Pattern
Send same operation to all shards for parallel execution.
See: `test_broadcast_to_all_shards`

### Pattern 4: Fire-and-Forget
Submit tasks that don't need to return results.
See: `test_submit_to_fire_and_forget`

### Pattern 5: Request-Response
Actor receives request with return channel for response.
See: `test_worker_actor_message_passing`

## Tips and Best Practices

1. **CPU Core Pinning**: Each shard is pinned to a specific CPU core for optimal cache locality
2. **Actor Isolation**: Actors on the same shard share no mutable state - use message passing
3. **Cross-Shard Cost**: `call_on` has overhead - batch work when possible
4. **Backpressure**: Actor mailboxes can fill up - design with flow control in mind
5. **Graceful Shutdown**: Always call `signal_monorail(Ok(()))` to cleanly exit the runtime

## Further Reading

- See `ARCHITECTURE.md` for detailed runtime internals
- See `README.md` for project overview
- See source code in `src/core/` for implementation details
