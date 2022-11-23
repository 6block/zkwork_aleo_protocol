# zkwork_aleo_protocol
Zk.Work Aleo Protocol is a set of ore pool protocols designed by 6Block for Aleo mining. This library is an open source implementation of the ZK. Work Aleo Protocol.
## specs
### roles
1. zk.work aleo worker
2. zk.work aleo pool agent
3. zk.work pool server
### message
1. connect server
  
   **<<128,worker_type, address_type, v_major, v_minor, v_patch, name_length, name, address>>**
2. submit solution

   **<< 129, worker_id, job_id, prover_solution >>**
3. disconnect server
   
   **<< 130, worker_id >>**

4. ping
   
   **<< 131 >>**
5. connect server ack
   
   **<< 0, is_accept, pool_address, [worker_id], [signature] >>**
6. notify job
   
   **<< 1, job_id, target, epoch_challenge >>**
7. pool shutdown

   **<< 2 >>**
8. pong
    
    **<< 3 >>**

## License

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](./LICENSE.md)