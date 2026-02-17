use crate::core::monolib::{self, actors};
use crate::core::{
    actor::base::{Actor, SelfAddr},
    topology::MonorailTopology,
    shard::{shard::signal_monorail, state::ShardId},
};
use futures::channel::oneshot;
use std::time::Duration;
use crate::core::topology::MonorailConfiguration;

/// A worker actor that processes data on a specific shard
struct WorkerActor;

#[derive(Debug)]
struct WorkItem {
    id: u64,
    data: Vec<u8>,
    result_tx: oneshot::Sender<ProcessedResult>,
}

#[derive(Debug)]
struct ProcessedResult {
    id: u64,
    checksum: u64,
    worker_shard: usize,
}

impl Actor for WorkerActor {
    type Message = WorkItem;
    type Arguments = ();
    type State = WorkerState;

    async fn pre_start(_args: Self::Arguments) -> Self::State {
        println!("[Worker on shard {:?}] Starting up", monolib::shard_id());
        WorkerState {
            processed_count: 0,
        }
    }

    async fn handle(
        _this: SelfAddr<'_, Self>,
        message: Self::Message,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        // Simulate some CPU-intensive work
        let checksum = message.data.iter().fold(0u64, |acc, &x| acc.wrapping_add(x as u64));
        
        state.processed_count += 1;
        
        println!(
            "[Worker on shard {:?}] Processed item {} (total: {})",
            monolib::shard_id(),
            message.id,
            state.processed_count
        );

        let result = ProcessedResult {
            id: message.id,
            checksum,
            worker_shard: monolib::shard_id().as_usize(),
        };

        let _ = message.result_tx.send(result);
        Ok(())
    }

    async fn post_stop(
        _this: SelfAddr<'_, Self>,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        println!(
            "[Worker on shard {:?}] Shutting down. Processed {} items total.",
            monolib::shard_id(),
            state.processed_count
        );
        Ok(())
    }
}

struct WorkerState {
    processed_count: u64,
}

/// A coordinator actor that distributes work across shards
struct CoordinatorActor;

struct CoordinateTask {
    num_items: u64,
    completion_tx: oneshot::Sender<Vec<ProcessedResult>>,
}

struct CoordinatorState {
    worker_shards: Vec<ShardId>,
}

impl Actor for CoordinatorActor {
    type Message = CoordinateTask;
    type Arguments = Vec<ShardId>;
    type State = CoordinatorState;

    async fn pre_start(args: Self::Arguments) -> Self::State {
        println!("[Coordinator] Starting with {} worker shards", args.len());
        CoordinatorState {
            worker_shards: args,
        }
    }

    async fn handle(
        _this: SelfAddr<'_, Self>,
        message: Self::Message,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        println!("[Coordinator] Distributing {} work items", message.num_items);
        
        let mut results = Vec::new();
        
        // Distribute work across shards in round-robin fashion
        for i in 0..message.num_items {
            let shard_idx = (i as usize) % state.worker_shards.len();
            let target_shard = state.worker_shards[shard_idx];
            
            // Submit work to the target shard
            let (tx, rx) = oneshot::channel();
            
            monolib::submit_to(target_shard, async move || {
                // Spawn a worker actor on the target shard if needed
                // For this example, we'll just do the work directly
                let data: Vec<u8> = (0..100).map(|x| ((i + x) % 256) as u8).collect();
                let checksum = data.iter().fold(0u64, |acc, &x| acc.wrapping_add(x as u64));
                
                let result = ProcessedResult {
                    id: i,
                    checksum,
                    worker_shard: monolib::shard_id().as_usize(),
                };
                
                let _ = tx.send(result);
            });
            
            // Collect the result
            if let Ok(result) = rx.await {
                results.push(result);
            }
        }
        
        println!("[Coordinator] All work completed. Collected {} results", results.len());
        let _ = message.completion_tx.send(results);
        
        Ok(())
    }

    async fn post_stop(
        _this: SelfAddr<'_, Self>,
        _state: &mut Self::State,
    ) -> anyhow::Result<()> {
        println!("[Coordinator] Shutting down");
        Ok(())
    }
}

/// A statistics collector that gathers results from all shards
struct StatsActor;

#[derive(Debug)]
enum StatsMessage {
    RecordResult(ProcessedResult),
    GetStats(oneshot::Sender<Statistics>),
}

#[derive(Debug, Clone)]
struct Statistics {
    total_processed: u64,
    per_shard_count: Vec<(usize, u64)>,
    total_checksum: u64,
}

struct StatsState {
    results: Vec<ProcessedResult>,
}

impl Actor for StatsActor {
    type Message = StatsMessage;
    type Arguments = ();
    type State = StatsState;

    async fn pre_start(_args: Self::Arguments) -> Self::State {
        println!("[Stats] Starting statistics collector");
        StatsState {
            results: Vec::new(),
        }
    }

    async fn handle(
        _this: SelfAddr<'_, Self>,
        message: Self::Message,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        match message {
            StatsMessage::RecordResult(result) => {
                state.results.push(result);
            }
            StatsMessage::GetStats(tx) => {
                // Calculate statistics
                let mut per_shard: std::collections::HashMap<usize, u64> = std::collections::HashMap::new();
                let mut total_checksum = 0u64;
                
                for result in &state.results {
                    *per_shard.entry(result.worker_shard).or_insert(0) += 1;
                    total_checksum = total_checksum.wrapping_add(result.checksum);
                }
                
                let mut per_shard_vec: Vec<_> = per_shard.into_iter().collect();
                per_shard_vec.sort_by_key(|(shard, _)| *shard);
                
                let stats = Statistics {
                    total_processed: state.results.len() as u64,
                    per_shard_count: per_shard_vec,
                    total_checksum,
                };
                
                println!("[Stats] Reporting: {:?}", stats);
                let _ = tx.send(stats);
            }
        }
        Ok(())
    }

    async fn post_stop(
        _this: SelfAddr<'_, Self>,
        state: &mut Self::State,
    ) -> anyhow::Result<()> {
        println!("[Stats] Shutting down. Processed {} results total", state.results.len());
        Ok(())
    }
}

/// Test 1: Cross-shard computation using call_on
/// Demonstrates submitting tasks to specific shards and collecting results
#[crate::test]
async fn cross_shard_computation() {
    
    let topology = monolib::get_topology_info();
    println!("Topology initialized with {} cores", topology.cores);
    
    let num_cores = topology.cores;
    let mut handles = Vec::new();
    
    // Submit tasks to all shards
    for i in 0..num_cores {
        let shard_id = ShardId::new(i);
        let handle = monolib::call_on(shard_id, move || async move {
            // Each shard computes a Fibonacci number
            let fib = compute_fibonacci(20 + i as u32);
            println!("Shard {} computed fib({}) = {}", 
                monolib::shard_id().as_usize(), 
                20 + i as u32,
                fib
            );
            fib
        });
        handles.push(handle);
    }
    
    // Wait for all results
    let mut total = 0u64;
    for handle in handles {
        if let Ok(result) = handle.await {
            total += result;
        }
    }
    println!("Total sum from all shards: {}", total);
    assert!(total > 0, "Should have computed some fibonacci numbers");
    
}

/// Test 2: Actor-based distributed processing
/// Demonstrates spawning actors, message passing, and coordination
#[crate::test]
async fn actor_based_processing() {
    
    let topology = monolib::get_topology_info();
    let num_cores = topology.cores;
    
    // Spawn a coordinator on shard 0
    let coordinator = monolib::spawn_actor::<CoordinatorActor>(
        (0..num_cores).map(|i| ShardId::new(i)).collect()
    ).register("coordinator").await;
    
    // Register the coordinator for discovery
    // let _ = actors::register("coordinator", coordinator.clone()).await;
    
    // Spawn a stats collector on shard 0
    let stats_actor = monolib::spawn_actor::<StatsActor>(());
    
    // Submit a coordination task
    let (tx, rx) = oneshot::channel();
    let _ = coordinator.send(CoordinateTask {
        num_items: 50,
        completion_tx: tx,
    }).await;
    
    // Wait for results
    if let Ok(results) = rx.await {
        println!("Received {} results from coordinator", results.len());
        assert_eq!(results.len(), 50, "Should have processed all 50 items");
        
        // Send results to stats collector
        for result in results {
            let _ = stats_actor.send(StatsMessage::RecordResult(result)).await;
        }
    }
    
    // Get final statistics
    let (stats_tx, stats_rx) = oneshot::channel();
    let _ = stats_actor.send(StatsMessage::GetStats(stats_tx)).await;
    if let Ok(stats) = stats_rx.await {
        println!("\n=== Final Statistics ===");
        println!("Total items processed: {}", stats.total_processed);
        println!("Per-shard distribution:");
        for (shard, count) in &stats.per_shard_count {
            println!("  Shard {}: {} items", shard, count);
        }
        println!("Total checksum: {}", stats.total_checksum);
        
        assert_eq!(stats.total_processed, 50, "Should have recorded all items");
    }
    
    // Clean shutdown
    coordinator.stop();
    stats_actor.stop();
    monolib::sleep(Duration::from_millis(50)).await;
    
}

/// Test 3: Broadcast to all shards using call_on_all
/// Demonstrates broadcasting a computation to all shards simultaneously
#[crate::test]
async fn broadcast_to_all_shards() {
    
    let topology = monolib::get_topology_info();
    println!("Broadcasting to {} shards", topology.cores);
    
    // Broadcast a task to all shards
    let results = monolib::call_on_all(|| async {
        // Each shard reports its ID
        let shard_id = monolib::shard_id().as_usize();
        println!("Shard {} responding", shard_id);
        shard_id
    }).await;
    
    println!("Received responses from {} shards:", results.len());
    assert_eq!(results.len(), topology.cores, "Should receive response from all shards");
    
    // Collect all shard IDs that responded
    let mut shard_ids = Vec::new();
    for result in results.iter() {
        match result {
            Ok(shard_id) => shard_ids.push(*shard_id),
            Err(e) => panic!("Error response: {:?}", e),
        }
    }
    
    // Sort and verify all shards responded exactly once
    shard_ids.sort();
    for (i, shard_id) in shard_ids.iter().enumerate() {
        println!("  Shard {} responded", shard_id);
        assert_eq!(*shard_id, i, "Each shard should respond exactly once");
    }
    
    
}

/// Test 4: Async operations and sleep
/// Demonstrates async sleep and timing operations
#[crate::test]
async fn async_operations() {
    
    println!("Current shard: {:?}", monolib::shard_id());
    
    // Test async sleep
    println!("Sleeping for 100ms...");
    let start = std::time::Instant::now();
    monolib::sleep(Duration::from_millis(100)).await;
    let elapsed = start.elapsed();
    println!("Awake after {:?}", elapsed);
    
    assert!(elapsed >= Duration::from_millis(95), "Should sleep for at least 95ms");
    assert!(elapsed < Duration::from_millis(200), "Should not sleep too long");
    
    // Test multiple concurrent sleeps
    println!("Testing concurrent operations...");
    let handles = (0..5).map(|i| {
        monolib::call_on(ShardId::new(i % monolib::get_topology_info().cores), move || async move {
            monolib::sleep(Duration::from_millis(10 * i as u64)).await;
            i as u64
        })
    }).collect::<Vec<_>>();
    
    for (i, handle) in handles.into_iter().enumerate() {
        if let Ok(result) = handle.await {
            assert_eq!(result, i as u64, "Should return correct value");
        }
    }
    
    println!("All concurrent operations completed");
    
    
}

/// Test 5: Submit tasks without waiting for results
/// Demonstrates fire-and-forget task submission
#[crate::test]
async fn fire_and_forget_task_submission() {
    
    let topology = monolib::get_topology_info();
    
    // Submit fire-and-forget tasks to all shards
    for i in 0..topology.cores {
        let shard_id = ShardId::new(i);
        monolib::submit_to(shard_id, move || async move {
            let fib = compute_fibonacci(15 + i as u32);
            println!("Shard {}: Computed fib({}) = {} (fire-and-forget)", 
                monolib::shard_id().as_usize(),
                15 + i as u32,
                fib
            );
        });
    }
    
    // Give tasks time to complete
    println!("Waiting for fire-and-forget tasks to complete...");
    monolib::sleep(Duration::from_millis(100)).await;
    println!("All fire-and-forget tasks should have completed");
    
    
}

/// Test 6: Simple actor lifecycle
/// Demonstrates basic actor creation, messaging, and shutdown
#[crate::test]
async fn simple_actor_lifecycle() {
    
    struct CounterActor;
    
    impl Actor for CounterActor {
        type Message = CounterMessage;
        type Arguments = i32;
        type State = i32;
        
        async fn pre_start(args: Self::Arguments) -> Self::State {
            println!("[Counter] Starting with initial value: {}", args);
            args
        }
        
        async fn handle(
            _this: SelfAddr<'_, Self>,
            message: Self::Message,
            state: &mut Self::State,
        ) -> anyhow::Result<()> {
            match message {
                CounterMessage::Increment => {
                    *state += 1;
                    println!("[Counter] Incremented to {}", *state);
                }
                CounterMessage::GetValue(tx) => {
                    println!("[Counter] Returning value {}", *state);
                    let _ = tx.send(*state);
                }
            }
            Ok(())
        }
        
        async fn post_stop(
            _this: SelfAddr<'_, Self>,
            state: &mut Self::State,
        ) -> anyhow::Result<()> {
            println!("[Counter] Shutting down with final value: {}", *state);
            Ok(())
        }
    }
    
    enum CounterMessage {
        Increment,
        GetValue(oneshot::Sender<i32>),
    }
    
    // Spawn a counter actor
    let counter = monolib::spawn_actor::<CounterActor>(10);
    
    // Increment it a few times
    for i in 1..=5 {
        let _ = counter.send(CounterMessage::Increment).await;
        println!("Sent increment #{}", i);
    }
    
    // Get the final value
    let (tx, rx) = oneshot::channel();
    let _ = counter.send(CounterMessage::GetValue(tx)).await;
    
    if let Ok(value) = rx.await {
        println!("Final counter value: {}", value);
        assert_eq!(value, 15, "Counter should be 10 + 5 increments");
    }
    
    // Stop the actor
    counter.stop();
    monolib::sleep(Duration::from_millis(50)).await;
    
    
}

/// Test 7: Worker actor with message passing
/// Demonstrates the WorkerActor processing work items
#[crate::test]
async fn worker_actor_message_passing() {
    
    // Spawn a worker actor
    let worker = monolib::spawn_actor::<WorkerActor>(());
    
    // Send some work items
    let mut result_receivers = Vec::new();
    
    for i in 0..5 {
        let (result_tx, result_rx) = oneshot::channel();
        
        let work_item = WorkItem {
            id: i,
            data: vec![i as u8; 100],
            result_tx,
        };
        
        let _ = worker.send(work_item).await;
        result_receivers.push(result_rx);
    }
    
    // Collect results
    let mut total_checksum = 0u64;
    for (i, rx) in result_receivers.into_iter().enumerate() {
        if let Ok(result) = rx.await {
            println!("Work item {} completed on shard {}", 
                result.id, 
                result.worker_shard
            );
            total_checksum += result.checksum;
        } else {
            panic!("Failed to receive result for work item {}", i);
        }
    }
    
    println!("Total checksum from all work items: {}", total_checksum);
    assert!(total_checksum > 0, "Should have processed work items");
    
    // Stop the worker
    worker.stop();
    monolib::sleep(Duration::from_millis(50)).await;    
}

// Helper function for computation
fn compute_fibonacci(n: u32) -> u64 {
    if n <= 1 {
        return n as u64;
    }
    let mut a = 0u64;
    let mut b = 1u64;
    for _ in 2..=n {
        let c = a.wrapping_add(b);
        a = b;
        b = c;
    }
    b
}
