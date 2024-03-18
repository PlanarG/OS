# Lab 1: Scheduling

---

## Information

Name: 李思杰

Email: planarg@stu.pku.edu.cn

> Please cite any forms of information source that you have consulted during finishing your assignment, except the TacOS documentation, course slides, and course staff.

> With any comments that may help TAs to evaluate your work better, please leave them here

## Alarm Clock

### Data Structures

> A1: Copy here the **declaration** of every new or modified struct, enum type, and global variable. State the purpose of each within 30 words.

![image-20240317215148865](/Users/planarg/Library/Application Support/typora-user-images/image-20240317215148865.png)

我在 `Manager` 结构题里面添加了成员 `sleep_threads`，它记录了所有等待被唤醒的线程。每当一个线程调用 `sleep` 函数时，它就会被添加到这个列表中。此唤醒列表每 `tick` 都会被检查一次。

### Algorithms

> A2: Briefly describe what happens in `sleep()` and the timer interrupt handler.

`sleep` 函数内只干了两件事：将当前线程根据待唤醒的时间挂到 `manager` 的唤醒列表上，以及阻塞当前线程。在 timer interrupt 内，每当 `ticks` 增加 $1$，操作系统都会检查一下这个待唤醒的列表，拿出此时还未唤醒，且应唤醒时刻最靠前的线程与当前时刻比较。操作系统会把所有应该在这一时刻之前唤醒的线程全部唤醒。 

> A3: What are your efforts to minimize the amount of time spent in the timer interrupt handler?

`sleep_threads` 是 `map` 而非 `vector`，这样可以快速地拿出等待唤醒时刻最靠前的线程，且删除也很快，时间复杂度都是 $\log$ 级别的。

### Synchronization

> A4: How are race conditions avoided when `sleep()` is being called concurrently?

事实上 `sleep` 函数内不需要任何显式的预防措施，因为它只需要把当前线程挂到 `sleep_threads` 上，因此只要它被上了一个 `Mutex`，这个函数就是线程安全的。

> A5: How are race conditions avoided when a timer interrupt occurs during a call to `sleep()`?

理论上来讲应该是要在 `sleep` 的开头关掉 `interrupt`，再在结尾打开的。但是我思考了一下发现这好像并不必要，因为检查函数每次都会唤醒应在当前时刻之前唤醒的线程，而非恰好应该在当前时刻唤醒。因此如果这个线程在挂到 `sleep_threads` 之前被 `interrupt` 打断，它所表现出的行为也是正常的。

## Priority Scheduling

### Data Structures

> B1: Copy here the **declaration** of every new or modified struct, enum type, and global variable. State the purpose of each within 30 words.

![image-20240317220753508](/Users/planarg/Library/Application Support/typora-user-images/image-20240317220753508.png)

我搞了一个先进先出的优先队列，它用来存当前所有状态为 `ready` 的线程以及信号量中等待 `V` 的线程，总之就是所有需要确保优先唤醒优先级高的线程，且先进先出的地方。一开始我想要用更高效的堆实现，但是后来在 `donation` 的部分中我发现程序可能会修改处于 `Blocked` 或者 `Ready` 状态的线程的优先级，因此用堆会很危险。

![image-20240317221258204](/Users/planarg/Library/Application Support/typora-user-images/image-20240317221258204.png)

我修改了 `Condvar` 的定义，往结构体里面塞了正在等待的线程。这样可以确保 `notify_one` 唤醒的是优先级最高的线程。

![image-20240317221418849](/Users/planarg/Library/Application Support/typora-user-images/image-20240317221418849.png)

有 `donation` 的关系的线程一定组成了一棵树形的结构：每个线程一定只会至多等待一个线程释放锁，我把它记录在了 `dependency` 中。同时，`donated_priorities` 用于记录所有直接依赖于这个线程的那些线程的有效优先级（所以这个线程的有效优先级就是它本身的优先级和这个列表中的最大值的最大值）。我在获取锁和释放锁的时候小心地维护了这两个变量。

但这里有一个我不知道如何解决的问题：为了在只有线程 `Arc` 指针的前提之下修改这两个成员，它们必须被套在 `Mutex` 里面，然而我们想要修改的就是加锁的过程，因此在这里套一层 `Mutex` 令人费解。实际上这里的 `Mutex` 的唯一作用就是通过编译，程序运行过程中这两个变量的锁不会出现多线程同时申请的情况。

![image-20240317222131229](/Users/planarg/Library/Application Support/typora-user-images/image-20240317222131229.png)

`EBinaryHeap` 是一个可以支持删除的堆，原理是同时维护两个堆，第一个存放此时堆中的所有元素，第二个存放此时应该从第一个堆里面删除，但是还没有删掉的元素。它用来维护依赖于当前线程的那些线程的优先级。

![image-20240317222456439](/Users/planarg/Library/Application Support/typora-user-images/image-20240317222456439.png)

为了知道有哪些线程正在等待当前这个锁被释放，我在 `Sleep` 结构题内新增了成员 `waiter`。它用来维护上面提到的 `dependency` 以及 `donated_priority`。

> B2: Explain the data structure that tracks priority donation. Clarify your answer with any forms of diagram (e.g., the ASCII art).

![image-20240317224203477](/Users/planarg/Library/Application Support/typora-user-images/image-20240317224203477.png)

### Algorithms

> B3: How do you ensure that the highest priority thread waiting for a lock, semaphore, or condition variable wakes up first?

每次取出线程的时候，取出有效优先级最大的线程就可以了

> B4: Describe the sequence of events when a thread tries to acquire a lock. How is nested donation handled?

其实上面的图里面已经比较清楚了（）

1. 先关 `interrupt`
2. 如果 `handler` 存在，则连边并更新这条链
3. 进行 `P` 操作
4. 将此时其余正在等待的线程连向自己，更新自己的 `donated_priority`
5. 更新这个锁的 `holder`，将它设为自己
6. 打开 `interrupt`

> B5: Describe the sequence of events when a lock, which a higher-priority thread is waiting for, is released.

1. 关闭 `interrupt`
2. 断开这个锁的所有连边，并将 `holder` 设置为空
3. 进行 `V` 操作，此操作会将那个优先级更高的线程放入 `ready` 列表中，进行一次 `schedule`，切换到那个线程
4. 执行权回到这个线程后，打开 `interrupt`。

### Synchronization

> B6: Describe a potential race in `thread::set_priority()` and explain how your implementation avoids it. Can you use a lock to avoid this race?

我的程序会比较当前线程和在 `ready` 列表中的线程的优先级，如果列表中有线程优先级更高则进行一次 `schedule`。如果不关闭 `interrupt`，程序可能会多进行一次 `schedule`。应该可以开一个全局的锁，通过获取与释放这个全局的锁实现修改优先级与 `schedule` 的原子化（其实本质与关 `interrput` 相同）

## Rationale

> C1: Have you considered other design possibilities? You can talk about anything in your solution that you once thought about doing them another way. And for what reasons that you made your choice?

可以独立于线程之外维护一个全局的、表征线程之间的依赖关系的结构，而不是在线程内维护 `dependency` 以及 `donated_priority`.

但是对于全局变量的操作仍然需要放在 `Mutex` 中（即使知道对它的访问一定不会冲突），本质上并没有解决前面的锁里套锁的问题，而且这样一点也不优雅。所以我最后没有采用这种方案。
