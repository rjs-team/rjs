// Shamelessly stolen from https://github.com/gpjt/webgl-lessons/blob/master/lesson01/index.html

let fragment = `
		// sometimes it failes on this line with:
    // "syntax error, unexpected NEW_IDENTIFIER"
    //precision mediump float;
    void main(void) {
        gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
    }
`;

let vertex = `
    attribute vec3 aVertexPosition;
    uniform mat4 uMVMatrix;
    uniform mat4 uPMatrix;
    void main(void) {
        gl_Position = uPMatrix * uMVMatrix * vec4(aVertexPosition, 1.0);
    }
`;

function execModule(module, __file__) {
  var exports = module.exports = {};
  var __source__ = getFileSync(__file__);
  puts(__file__+" len: " + __source__.length);
  eval(__source__);
  return module.exports;
}
glMatrix = execModule({}, "examples/webgl/gl-matrix.js");
puts(JSON.stringify(Object.keys(glMatrix)));
mat4 = glMatrix.mat4;

alert = puts;


var gl;
function initGL(canvas) {
  try {
    gl = canvas.getContext("experimental-webgl");
    gl.viewportWidth = canvas.width = 1024;
    gl.viewportHeight = canvas.height = 768;
    puts(JSON.stringify(gl.__proto__));

    gl = new Proxy(gl, {
      get: function(target, name) {
        if (name in target) {
          let val = target[name];
          if (val instanceof Function)
            return function() {
							let ret;
							try {
								ret = val.apply(target, arguments);
							} catch (e) {
								puts("gl."+name+"(" + Array.prototype.slice.call(arguments).map(x => "" + x).join(', ') + ")");
								puts("" + e);
								throw e;
							}
              let err = target.getError();
              puts("gl."+name+"(" + Array.prototype.slice.call(arguments).map(x => "" + x).join(', ') + "): " + ret);
              if (err != 0) {
                puts("GL error: " + err.toString(16));
              }
              return ret;
            };
          return val;
        }
        puts("Missing property: " + name);
        return function(){};
      },
    });
  } catch (e) {
  }
  if (!gl) {
    alert("Could not initialise WebGL, sorry :-(");
  }
}
function getShader(gl, type, source) {
  /*var shaderScript = document.getElementById(id);
  if (!shaderScript) {
    return null;
  }
  var str = "";
  var k = shaderScript.firstChild;
  while (k) {
    if (k.nodeType == 3) {
      str += k.textContent;
    }
    k = k.nextSibling;
  }*/
  var shader;
  if (type == "fragment") {
    shader = gl.createShader(gl.FRAGMENT_SHADER);
  } else if (type == "vertex") {
    shader = gl.createShader(gl.VERTEX_SHADER);
  } else {
    return null;
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    alert(gl.getShaderInfoLog(shader));
    return null;
  }
  return shader;
}
var shaderProgram;
function initShaders() {
  var fragmentShader = getShader(gl, "fragment", fragment);
  var vertexShader = getShader(gl, "vertex", vertex);
  shaderProgram = gl.createProgram();
  gl.attachShader(shaderProgram, vertexShader);
  gl.attachShader(shaderProgram, fragmentShader);
  gl.linkProgram(shaderProgram);
  if (!gl.getProgramParameter(shaderProgram, gl.LINK_STATUS)) {
    alert("Could not initialise shaders");
  }
  gl.useProgram(shaderProgram);
  shaderProgram.vertexPositionAttribute = gl.getAttribLocation(shaderProgram, "aVertexPosition");
  gl.enableVertexAttribArray(shaderProgram.vertexPositionAttribute);
  shaderProgram.pMatrixUniform = gl.getUniformLocation(shaderProgram, "uPMatrix");
  shaderProgram.mvMatrixUniform = gl.getUniformLocation(shaderProgram, "uMVMatrix");
}
var mvMatrix = mat4.create();
var pMatrix = mat4.create();
function setMatrixUniforms() {
  gl.uniformMatrix4fv(shaderProgram.pMatrixUniform, false, pMatrix);
  gl.uniformMatrix4fv(shaderProgram.mvMatrixUniform, false, mvMatrix);
}
var triangleVertexPositionBuffer;
var squareVertexPositionBuffer;
function initBuffers() {
  triangleVertexPositionBuffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, triangleVertexPositionBuffer);
  var vertices = [
     0.0,  1.0,  0.0,
    -1.0, -1.0,  0.0,
     1.0, -1.0,  0.0
  ];
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.STATIC_DRAW);
  triangleVertexPositionBuffer.itemSize = 3;
  triangleVertexPositionBuffer.numItems = 3;
  squareVertexPositionBuffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, squareVertexPositionBuffer);
  vertices = [
     1.0,  1.0,  0.0,
    -1.0,  1.0,  0.0,
     1.0, -1.0,  0.0,
    -1.0, -1.0,  0.0
  ];
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.STATIC_DRAW);
  squareVertexPositionBuffer.itemSize = 3;
  squareVertexPositionBuffer.numItems = 4;
}
function drawScene() {
  gl.viewport(0, 0, gl.viewportWidth, gl.viewportHeight);
  gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
  mat4.perspective(pMatrix, 45, gl.viewportWidth / gl.viewportHeight, 0.1, 100.0);
  mat4.identity(mvMatrix);
  mat4.translate(mvMatrix, mvMatrix, [-1.5, 0.0, -7.0]);
  gl.bindBuffer(gl.ARRAY_BUFFER, triangleVertexPositionBuffer);
  gl.vertexAttribPointer(shaderProgram.vertexPositionAttribute, triangleVertexPositionBuffer.itemSize, gl.FLOAT, false, 0, 0);
  setMatrixUniforms();
  gl.drawArrays(gl.TRIANGLES, 0, triangleVertexPositionBuffer.numItems);
  mat4.translate(mvMatrix, mvMatrix, [3.0, 0.0, 0.0]);
  gl.bindBuffer(gl.ARRAY_BUFFER, squareVertexPositionBuffer);
  gl.vertexAttribPointer(shaderProgram.vertexPositionAttribute, squareVertexPositionBuffer.itemSize, gl.FLOAT, false, 0, 0);
  setMatrixUniforms();
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, squareVertexPositionBuffer.numItems);
}
function webGLStart() {
  //var canvas = document.getElementById("lesson01-canvas");
  let canvas = new Window();
  canvas.onevent = function(event) {
    //puts("event: " + JSON.stringify(event));
  };
  initGL(canvas);
  initShaders();
  initBuffers();
  gl.clearColor(0.0, 0.1, 0.0, 1.0);
  gl.enable(gl.DEPTH_TEST);
  drawScene();
}



webGLStart();
