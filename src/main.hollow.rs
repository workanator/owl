/*
 * Copyright 2019 Andrew "workanator" Bashkatov
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::net::UdpSocket;

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:9090").unwrap();
    loop {
        let mut buf = [0; 512];
        let (_, src) = socket.recv_from(&mut buf).unwrap();
        println!("{} -> {}", src,  String::from_utf8_lossy(&buf));
    }
}
