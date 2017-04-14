puts("asdf");
setTimeout(function() { setTimeout(function() { puts("2!"); }, 100); puts("delayed!"); }, 1000);
setTimeout(function() { puts("3"); }, 100);
setTimeout(function() { puts("3"); }, 1100);
setTimeout(function() { puts("3"); }, 1100);
setTimeout(function() { puts("3"); throw new Error(); }, 1100);
setTimeout(function() { puts("4"); }, 1000);

puts(getFileSync("test.js"));
puts(readDir("."));

1 + 2 + Object
