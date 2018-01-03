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

function changeClear() {
  if (!window) return;

  window.clearColor(1, 0, 0, 1);
  window.clear();
}

function ping() {
  if (window) window.ping();
  else return puts("puts: window == null");

  setTimeout(ping, 500);
}

setTimeout(function() {
  window = new Window();
  window.onevent = function(event) {
    puts("Event! " + JSON.stringify(event));
  };

  ping();
  setTimeout(changeClear, 1000);

  setTimeout(function() {
    puts("Wait over... closing...");
    window.close();

    setTimeout(function() {
      puts("Second wait...");
      window = null;
    }, 10000);
  }, 10000);
}, 1000);


1 + 2 + Object

