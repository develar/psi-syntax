using System;
using System.Runtime.InteropServices;

interface IGoo
{
    [return: MarshalAs(UnmanagedType.I2)]
    short M();

    int Prop
    {
        [return: MarshalAs(UnmanagedType.I4)]
        get;
        [param: MarshalAs(UnmanagedType.I4)]
        set;
    }

    long this[[MarshalAs(UnmanagedType.BStr)] string s]
    {
        [return: MarshalAs(UnmanagedType.I8)]
        get;
        set;
    }
}

public class Goo
{
    public delegate void MyDelegate();
    public event MyDelegate eventMethod
    {
        [method: ComVisible(true)]
        add { }
        remove { }
    }
}