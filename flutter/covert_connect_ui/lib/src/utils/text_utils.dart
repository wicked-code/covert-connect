String toDataSize(BigInt bytes, [bool speed = false]) {
    final step = BigInt.from(1000);
    final sizes = speed ? ['B/s', 'kB/s', 'MB/s', 'GB/s', 'TB/s'] : ['B', 'kB', 'MB', 'GB', 'TB'];

    int idx = 0;
    while(idx < sizes.length - 1 && bytes >= step) {
      idx++;
      bytes = bytes ~/ step; 
    }

    return "$bytes ${sizes[idx]}";
}
