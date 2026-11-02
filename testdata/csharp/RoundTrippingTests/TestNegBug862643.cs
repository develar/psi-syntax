
using System;
[AttributeUsage(AttributeTargets.All)]
public class Goo : Attribute
    {
    public int Name;
    public Goo (int sName) {Name = sName;}
    }
public class Class1 {
 int Meth2 ([event:Goo(5)]int parm) {return 0;}
 public int IP { get {return 0;} set {}}
}
