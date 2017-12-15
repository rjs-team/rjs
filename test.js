puts("asdf");
setTimeout(function() { setTimeout(function() { puts("2!"); }, 100); puts("delayed!"); }, 1000);
setTimeout(function() { puts("3"); }, 100);
setTimeout(function() { puts("3"); }, 1100);
setTimeout(function() { puts("3"); }, 1100);
setTimeout(function() { puts("3"); throw new Error(); }, 1100);
setTimeout(function() { puts("4"); }, 1000);

puts(getFileSync("test.js"));
puts(readDir("."));

try {
	puts(1, 2);
} catch (e) {
	puts('Caught error: '+e);
}

var t = new Test();
puts("Test: " + Object.keys(Test.prototype) + ";");
t.test_puts(t.test_prop);

puts("Globals: " + Object.keys(this));

let window;

setTimeout(function() {
  window = new Window();
  window.ping();
  setTimeout(function() {
    puts("Wait over...");
    window = null;

    setTimeout(function() {
      puts("Second wait...");
    }, 10000);
  }, 10000);
}, 1000);


1 + 2 + Object

