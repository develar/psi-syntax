using System;
class AAttribute : Attribute {
  public AAttribute(object o) { }
}
[A(new string[] is { "hello" })]
class C {
}
