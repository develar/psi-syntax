using System;

[AttributeUsage(AttributeTargets.All)]
public class X : Attribute { }
[@X]
class A { }

namespace @namespace
{
    class C1 { }
    class @class : C1 { }
}

namespace N2
{
    class Test
    {
        static int Main()
        {
            global::@namespace.@class c1 = new global::@namespace.@class();
            global::@namespace.C1 c2 = new global::@namespace.@C1();

            return 0;
        }
    }
}