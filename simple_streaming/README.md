About  
A simple baby cam server

Requirements
 - Rust build tools
 - OpenCV 4.6.0

How to Build
 - cargo build --release

set OPENCV_LINK_LIBS if cargo fails to find OpenCV

ex on nu  
with-env { OPENCV_LINK_PATHS:"/path/to/opencv/build/lib"} { cargo build --release --target aarch64-unknown-linux-gnu }
