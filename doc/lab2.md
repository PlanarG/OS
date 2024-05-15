# Lab 2: User Programs

---

## Information

Name: 李思杰

Email: 2200012943@stu.pku.edu.cn

> Please cite any forms of information source that you have consulted during finishing your assignment, except the TacOS documentation, course slides, and course staff.

> With any comments that may help TAs to evaluate your work better, please leave them here

## Argument Passing

#### DATA STRUCTURES

> A1: Copy here the **declaration** of each new or changed struct, enum type, and global variable. State the purpose of each within 30 words.

在这个 task 中我没有修改现有的数据结构，只有对于现有函数的略微修改。

#### ALGORITHMS

> A2: Briefly describe how you implemented argument parsing. How do you arrange for the elements of argv[] to be in the right order? How do you avoid overflowing the stack page?

由于新创建的页表还没有激活，所以我只能通过 `kernel` 页表访问到新创建程序的栈空间。因此我修改了 `load_executable` 函数以及 `init_user_stack` 函数，让它们可以返回在新创建出的页表中栈指针对应的物理地址在 `kernel` 页表中对应的虚拟地址 $p$。通过访问 $p$ 就能够实现修改新线程的栈空间。

我按照 `pintos` 文档中对 `argv` 的排布顺序要求调整并对齐了传入的参数：首先从后往前将 `argv` 中的字符串放入栈中，同时记录每个字符串开始位置的地址；接下来先空出 $8$ 位，将这 $8$ 位覆盖为 $0$，表示 `argv[argc] = 0`，然后再依次放入在上一步记录下的地址。最后更新寄存器。

由于在这个 `lab` 中用于测试的程序使用的栈空间都不大，因此 `4KB` 的栈空间是够用的。为了避免栈溢出，我在使用指针之前都根据当前页表检查了一下这个指针指向的地址是否合法，访问权限是否足够。

#### RATIONALE

> A3: In Tacos, the kernel reads the executable name and arguments from the command. In Unix-like systems, the shell does this work. Identify at least two advantages of the Unix approach.

- `shell` 可以先检查传入参数以及指针的合法性，再将检查完毕的命令传递给内核，这样会更安全。
- 可以通过自定义一些配置文件调整 `shell` 解析可执行文件名，或者解析参数的行为，例如添加环境变量。这样对于用户来说自由度更高，更加方便。

## System Calls

#### DATA STRUCTURES

> B1: Copy here the **declaration** of each new or changed struct, enum type, and global variable. State the purpose of each within 30 words.

![image-20240411111035875](/Volumes/Workspace/rust/Tacos/doc/lab2/image-20240411111035875.png)

`ChildStatus` 的作用是记录当前线程创建出的子线程的状态：正在运行或者是已终止，便于 `wait` 中进行条件判断以及资源回收。

在 `Thread` 中新建了 `children`，`descriptors` 两个变量。其中 `children` 用于记录当前线程的所有子线程，`descriptors` 用于记录当前线程使用的所有文件描述符。

![image-20240411111206852](/Volumes/Workspace/rust/Tacos/doc/lab2/image-20240411111206852.png)

对于每个线程记录它的父线程 `parent`，这样当程序退出或者被创建时可以及时更新父线程的状态。

> B2: Describe how file descriptors are associated with open files. Are file descriptors unique within the entire OS or just within a single process?

`File` 结构题本身已经内置了多线程对同一文件的访问逻辑，因此实际上每个程序对文件的访问都可以近似看作 `exclusive` 的。当当前线程尝试打开一个文件时，内核会找到除了 `0, 1, 2` ，以及当前线程已经使用过的文件标识符以外的一个数作为当前文件的 `fd`，并在 `descriptors` 中将这个标识符与文件关联起来。后续这个线程对于这个标识符的所有访问都可以通过 `descriptors` 定位到对应的文件。

`descriptors` 对于各线程来说是独立的。

#### ALGORITHMS

> B3: Describe your code for reading and writing user data from the kernel.

对于用户传入的指针，`kernel` 会先通过 pagetable 获得这个指针对应的 `entry`，检查这个 `entry` 是否存在以及对应的权限位。对于传入的 `buffer`，它的开头和结尾都会被检查。在检查完毕之后 `kernel` 会通过这个指针向它指向的地址读取或者写入数据。

> B4: Suppose a system call causes a full page (4,096 bytes) of data to be copied from user space into the kernel. 
> What is the least and the greatest possible number of inspections of the page table 
> (e.g. calls to `Pagetable.get_pte(addr)` or other helper functions) that might result?
> What about for a system call that only copies 2 bytes of data?
> Is there room for improvement in these numbers, and how much?

最少检查 $2$ 次，最多检查 $4096$ 次。检查 $2$ 次是因为在一次性复制 `4KB` 的条件下必须检查头尾确保安全性，而 $4096$​ 则是将每个 `byte` 都检查了一遍。  

如果只复制 `2B` 的数据，`kernel` 应该也会检查 $2$ 次：即依次检查传入的这两个字节是否合法。

如果每次用户复制的都是完整的一页，那么我们只需要检查这一页起始位置对应的地址是否合法：页表是以页为最小单位分配地址的，那么这一页中只要有一个地址合法，其余所有地址也一定合法。这样可以将检查次数减少到 $1$。

> B5: Briefly describe your implementation of the "wait" system call and how it interacts with process termination.

`wait` 中当前线程会持续地检查目标线程有没有退出，并且每次检查都是原子过程。我的实现可以分为以下几个步骤：

- 检查目标线程是否是当前线程的子线程。
- 对 `children` 上锁，检查这个线程是否退出，检查完毕之后释放锁。
- 如果已经退出，则更新 `children`，释放相应资源，否则调用 `schedule` 进行等待，在执行权切回当前线程后重新判断上面的条件。

如果在 `wait` 的过程之中子线程终止，那么它相应的 `status` 会在 `exit` 函数中更新到父线程的 `children` 中，这样父线程就可以知道子线程已经终止，从而在函数中及时退出循环。

> B6: Any access to user program memory at a user-specified address
> can fail due to a bad pointer value.  Such accesses must cause the
> process to be terminated.  System calls are fraught with such
> accesses, e.g. a "write" system call requires reading the system
> call number from the user stack, then each of the call's three
> arguments, then an arbitrary amount of user memory, and any of
> these can fail at any point.  This poses a design and
> error-handling problem: how do you best avoid obscuring the primary
> function of code in a morass of error-handling?  Furthermore, when
> an error is detected, how do you ensure that all temporarily
> allocated resources (locks, buffers, etc.) are freed?
> Have you used some features in Rust, to make these things easier than in C?
> In a few paragraphs, describe the strategy or strategies you adopted for
> managing these issues.  Give an example.

对于线程级别的资源，由于对于每个线程，无论它是正常执行完毕还是异常退出，`exit` 函数都会被执行。因此，我们可以在 `exit` 函数中对当前线程使用的资源进行释放。例如，更新父线程中与这个子线程相关联的数据。

相对于 C，Rust 有更为完善的垃圾回收机制：变量会在生命周期结束，或者没有指向它的任何引用时被销毁，`drop` 函数会被自动调用，因此可以将资源回收过程放入 `drop` 函数中。

比如 `Mutex`，它会在 `drop` 的过程之中自动释放锁，因此即使当前线程获取了某个 `Mutex` 且在拥有锁的过程之中异常退出，`Mutex` 也会在 `drop` 的时候自动释放，此时其余线程仍然可以正常执行。但是，对于信号量则没有这样的机制：当前线程因为异常退出而没有调用 `V`，那么其余线程只能一直等待，毕竟信号量是多对多的。

对于文件，`inode` 内会维护每个文件的访问次数，在文件没有任何引用的时候将它从内存中移除，释放空间；在启动线程时会将其对应的 `File` 设置为不可写，由于 `File` 在 `drop` 函数内会自动地将当前文件设置为可写，这样也可以使得：如果进程 `A` 调用了进程 `B`，进程 `B` 发生运行时错误导致异常退出，那么进程 `B` 对应的文件仍然能够正常访问，而不是被永久地设置为只读。  

> B7: Briefly describe what will happen if loading the new executable fails. (e.g. the file does not exist, is in the wrong format, or some other error.)

解析目标文件名、解析传入参数、加载 `elf` 文件并初始化栈空间、启动新线程的任意一个阶段发生错误，都会导致 `execute` 函数返回 `-1`，此时没有任何数据结构会修改。

#### SYNCHRONIZATION

> B8: Consider parent process P with child process C.  How do you
> ensure proper synchronization and avoid race conditions when P
> calls wait(C) before C exits?  After C exits?  How do you ensure
> that all resources are freed in each case?  How about when P
> terminates without waiting, before C exits?  After C exits?  Are
> there any special cases?

当 `C` 退出时，它会通知 `P` 自己已经退出了，因此 `P` 会将 `C` 的状态设置为 `exit`。当 `P` 调用 `wait` 时，如果 `C` 已经退出，那么 `P` 可以从子线程的状态表中得知 `C` 已经退出，此时它会清空 `children` 里面线程 `C` 对应的那一项，并返回 `C` 线程的返回值。 

如果 `P` 在 `C` 退出之前调用 `wait`，那么在条件判断的时候 `P` 就会发现 `C` 其实并没有退出，此时 `P ` 会交出执行权，一直等待直到 `C` 退出，然后再释放 `children` 里面 `C` 对应的那一项。

如果 `P` 没有通过 `wait` 来回收 `C` 的数据，那么在 `P` 退出的时候，`P` 内部的数据会由于没有任何引用的 `Arc` 指针被释放，此时 `C` 对应的数据也会被释放。

如果 `P` 先于 `C` 之前退出，且没有通过 `wait` 回收 `C` 的资源，那么 `C` 会变成一个孤儿线程：它不是任何线程的子线程，占用的空间不会被释放，也能一直保持运行直至退出。

#### RATIONALE

> B9: Why did you choose to implement access to user memory from the
> kernel in the way that you did?

用户程序占有的用户虚拟地址是连续的，因此对于传入的数组我们只需要检查头尾两个指针是否合法，就可以判断中间的这一段地址是否全部都是已经分配的用户地址。

> B10: What advantages or disadvantages can you see to your design
> for file descriptors?

我采用了 `BTreeMap` 维护所有的文件描述符，这样的优点是很节约内存，即使当前线程反复申请与释放文件描述符，所占用的空间都始终只与当前打开的文件数目有关。但缺点是每次访问一个文件描述符都需要 $O(\log n)$ 的时间，相对于 $O(1)$ 的线性表来说不够高效。

> B11: What is your tid_t to pid_t mapping. What advantages or disadvantages can you see to your design?

我直接让这两相等了，这样的好处是在 `lab` 中我们不需要考虑单进程多线程的场景，因此实现起来很简单，但是缺点是不支持多线程。
