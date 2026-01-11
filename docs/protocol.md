# Protocol

## message

### request

    nonce                         | not encrypted
    salt 
    padding                 u16   |  size of padding after message
    host len                u8    |  host name string length
    tag                           |  aead tag of ChaCha20Poly1305 or Aes256Gcm (depens on config)

    host name                     |  host name string
    tag                           |  aead tag of ChaCha20Poly1305 or Aes256Gcm (depens on config)

    padding                       |  random padding, not encrypted

## special message (get protocol config)

### request

    nonce                         | not encrypted
    salt 
    padding                 u16   |  size of padding after message
    tag                           |  aead tag of Aes256Gcm
    tag                           |  aead tag of ChaCha20Poly1305
    random padding

### response

    padding_begin           u16   |  size of padding before payload
    padding_end             u16   |  size of padding after payload
    tag                           |  aead tag of Aes256Gcm
    tag                           |  aead tag of ChaCha20Poly1305
    padding                       |  random padding (begin)
    payload      
    padding                       |  end padding
    tag                           |  aead tag of Aes256Gcm
    tag                           |  aead tag of ChaCha20Poly1305
    
Client can't know kdf, cipher and max_connect_delay thus select values as default!
kdf - Argon2 as strongest, and speed most probably is not so important here
cipher - just use both Aes256Gcm and ChaCha20Poly1305... can be fixed if we implement temp keys (include in key itself)
max_connection_delay - 30 sec, shoud be enough... can be fixed if we implement temp keys (include in key itself)
header padding is bigger since this function is rarely used
