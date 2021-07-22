use std::io::{Read, Result as IoResult, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, LockResult, Mutex, MutexGuard};
use std::thread;

fn main() {
    println!("Started: Echo Server!");

    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    let thread_pool = ThreadPool::new(8);

    for tcp in listener.incoming() {
        match tcp {
            Ok(stream) => {
                thread_pool.execute(|| {
                    handle(stream);
                });
            }
            Err(e) => {
                println!("Could not establish connection due to: {:?}", e);
            }
        }
    }
}

fn handle(mut stream: TcpStream) {
    let mut buffer = [0u8; 1024];

    loop {
        match echo(&mut stream, &mut buffer) {
            Ok(read_bytes) if read_bytes == 0 => {
                println!("All bytes were read!");
                break;
            }
            Err(e) => {
                println!("Stopping further processing of stream due to: {:?}", e);
                break;
            }
            _ => {}
        }
    }
}

fn echo(stream: &mut TcpStream, buffer: &mut [u8]) -> IoResult<usize> {
    let read_bytes = stream.read(buffer)?;
    stream.write(&buffer[0..read_bytes])?;
    Ok(read_bytes)
}

enum Operation {
    Execute(Task),
    Terminate,
}

type Task = Box<dyn FnOnce() + Send + 'static>;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Operation>,
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver: SharedReceiver = Arc::new(Mutex::new(receiver));

        let workers: Vec<Worker> = (0..size)
            .map(|id| Worker::new(id, Arc::clone(&receiver)))
            .collect();
        ThreadPool { workers, sender }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let task = Box::new(f);

        if let Err(e) = self.sender.send(Operation::Execute(task)) {
            println!(
                "Request rejected, could not enqueue new task, reason: {:?}",
                e
            );
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        println!("Terminating thread pool responsible for request processing");

        for _ in 0..self.workers.len() {
            self.sender.send(Operation::Terminate).unwrap();
        }

        for worker in &mut self.workers {
            if let Some(worker) = worker.thread.take() {
                worker.join().unwrap();
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

type SharedReceiver = Arc<Mutex<mpsc::Receiver<Operation>>>;

impl Worker {
    pub fn new(id: usize, receiver: SharedReceiver) -> Worker {
        let thread = thread::spawn(move || loop {
            if let Some(op_res) = receiver.lock().ok().map(|r| r.recv()) {
                match op_res {
                    Ok(operation) => match operation {
                        Operation::Execute(task) => {
                            println!("Worker {} starts processing new request", id);
                            task();
                        }
                        Operation::Terminate => {
                            println!("Worker {} received terminate signal", id);
                            break;
                        }
                    },
                    Err(e) => {
                        println!("Could not establish connection due to: {:?}", e);
                    }
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
