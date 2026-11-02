using System;
public class Test
{
public static int Main()
{
int i = 0;
#if false
aaa
#if true
this shouldn't even be looked at.
#else
Except for counting up the #if/ #endif pairs
#endif
This should be skipped too!
#endif
return (i);
}
}
