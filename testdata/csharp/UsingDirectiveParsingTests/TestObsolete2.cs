using System;
using X = C[];

[Obsolete("", error: true)]
class C
{
}

class D
{
    X x1;
    C[] c1;
}