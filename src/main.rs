// #![deny(unsafe_code)]
#![feature(proc_macro)]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt;
extern crate cortex_m_rtfm as rtfm;
extern crate stm32f40x;

use cortex_m::*;
use cortex_m::peripheral::SystClkSource;
use rtfm::{app, Resource, Threshold};

app! {
    device: stm32f40x,
    resources: {
        static ON: bool = false;                            // Holds bool for led
        static INPUT_COMMAND: [u8; 100] = [0 as u8; 100];   // Store usart characters in array to be processed when executing
        static INPUT_COMMAND_POSITION: u8 = 0;              // Current position in array
        static WORKING_TIME: u32 = 0;                       // Stores time spent working
        static SLEEPING_TIME: u32 = 0;                      // Stores time spent sleeping
        static LAST_TIME: u32 = 0;                          // Stores last time work started
    },
    tasks: {
        SYS_TICK:
        {
            path: itm_update,
            resources: [ITM, WORKING_TIME, SLEEPING_TIME],
        },
        TIM2: {
            path: switch,
            resources: [GPIOA, ON, TIM2],
        },
        USART2:
        {
            path: loopback,
            resources: [TIM2 ,USART2, INPUT_COMMAND, INPUT_COMMAND_POSITION]
        },
    },

    idle: 
    {
        resources: [DWT, SLEEPING_TIME, WORKING_TIME, LAST_TIME]
    }
}

fn init(p: init::Peripherals, _r: init::Resources)
{
    // Power up GPIOA
    p.RCC.ahb1enr.modify(|_, w| w.gpioaen().set_bit());
    // Set as output
    p.GPIOA.moder.modify(|_, w| w.moder5().bits(0b01));

    // Set clock source to system clock
    p.SYST.set_clock_source(SystClkSource::Core);
    // 16 MHz = 16_000_000 for 1 second intervals
    p.SYST.set_reload(16000000); // 1s
    // Enable syst interrupt
    p.SYST.enable_interrupt();
    // Enable syst counter
    p.SYST.enable_counter();

    // Power up TIM2
    p.RCC.apb1enr.modify(|_, w| w.tim2en().set_bit());
    // Prescaler 64 * 250000 = 1s
    let cnt_value_tim2: u32 = 250000;
    unsafe {
    p.TIM2.arr.write(|w| w.bits(cnt_value_tim2));
    }
    let psc_value_tim2: u16 = 64;
    unsafe {
    p.TIM2.psc.write(|w| w.psc().bits(psc_value_tim2));
    }
    // Enable TIM2 interrupt
    p.TIM2.dier.write(|w| w.uie().set_bit());
    // Reset TIM2 on interupt
    p.TIM2.egr.write(|w| w.ug().set_bit());
    // Enable counter for TIM2
    p.TIM2.cr1.write(|w| w.cen().bit(true));
    // Auto reload TIM2 on interupt
    p.TIM2.cr1.write(|w| w.arpe().set_bit());

    // Power up USART2
    p.RCC.apb1enr.modify(|_, w| w.usart2en().set_bit());
    // Set output pins to alternate function
    p.GPIOA.moder.modify(|_, w| w.moder2().bits(2).moder3().bits(2));
    // Set alternat function as USART2
    p.GPIOA.afrl.write(|w| w.afrl2().bits(7).afrl3().bits(7));

    // Enable usart, enable transmission, enable receive, enable receive interupt
    p.USART2.cr1.write(|w| w.ue().set_bit().te().set_bit().re().set_bit().rxneie().set_bit());

    // Set baud to the core clock (16.000.000) divided by the needed baudrate(115.200)
    let baud = 16000000 / 115200;
    unsafe {
    p.USART2.brr.write(|w| w.bits(baud));
    }

    // DWT cycle counter to be used in CPU UTILIZATION as per https://stm32f4-discovery.net/2015/05/cpu-load-monitor-for-stm32f4xx/ with some changes
    unsafe {
    p.DWT.enable_cycle_counter();
    p.DWT.cyccnt.write(0);
    }

}

fn switch(t: &mut Threshold, r: TIM2::Resources)
{
    // remove interupt bit and enable counter again.
    r.TIM2.claim_mut(t, |tim2, _t| 
    {
        tim2.cr1.write(|w| w.cen().bit(true));
        tim2.sr.write(|w| w.uif().bit(false));
    });

    r.ON.claim_mut(t, |on, _t| 
    {
        **on = !**on;
    });
    if **r.ON
    {
        r.GPIOA.claim_mut(t, |gpioa, _t| 
        {
            gpioa.odr.write(|w| w.odr5().bit(true));
        });
    }
    else 
    {
        r.GPIOA.claim_mut(t, |gpioa, _t| 
        {
            gpioa.odr.write(|w| w.odr5().bit(false));
        });
    }

}

fn idle(t: &mut Threshold, mut r: ::idle::Resources) -> !
{   
    loop {
        // Disable interupts to not count interupt handling in time spent sleeping
        cortex_m::interrupt::disable();

        // Calulates time spent working by measuring difference from current count to counter when work started saved in LAST_TIME
        let last_time = *r.LAST_TIME;
        let new_count_before = r.DWT.cyccnt.read();
        r.WORKING_TIME.claim_mut(t, |working_time,_t|
        {
            **working_time += new_count_before - last_time;
        });

        // Calculate time spent sleeping by saving count before sleep and count when interupt occured.
        let count_before: u32 = r.DWT.cyccnt.read();
        rtfm::wfi();
        let new_count = r.DWT.cyccnt.read();
        r.SLEEPING_TIME.claim_mut(t, |sleeping_time, _t | 
        {
            **sleeping_time += new_count - count_before;
        });

        // Set count when work starts and enable interupt so interupt handler can take affect.
        *r.LAST_TIME = r.DWT.cyccnt.read();
        unsafe {
        cortex_m::interrupt::enable();
        }
    }
}

fn itm_update(t: &mut Threshold, r: SYS_TICK::Resources)
{
    // Simple calculation for cpu utilization according to https://stm32f4-discovery.net/2015/05/cpu-load-monitor-for-stm32f4xx/ 
    let work : f32 = **r.WORKING_TIME as f32;
    let sleep : f32 = **r.SLEEPING_TIME as f32;
    let usage : f32 = (work / (sleep + work))*100.0;

    r.ITM.claim_mut(t, |itm, _t| 
    {
    iprintln!(&itm.stim[0],"{}% is the CPU usage", usage);
    });

    // Reset time spent sleeping and working to get new values for CPU utilization.
    r.SLEEPING_TIME.claim_mut(t, |sleeping_time, _t| 
    {
        **sleeping_time = 0;
    });
    r.WORKING_TIME.claim_mut(t, |working_time, _t| 
    {
        **working_time = 0;
    });
}

fn loopback(t: &mut Threshold, r: USART2::Resources)
{   
    let received_character = r.USART2.dr.read().bits() as u8;   // Read character from USART data register
    handle_input(t, r.INPUT_COMMAND_POSITION, r.INPUT_COMMAND, r.USART2, received_character, r.TIM2);

    // Echo back character through usart
    r.USART2.claim_mut(t, |usart2, _t| {
    unsafe 
    {
        usart2.dr.write(|w| w.dr().bits(received_character as u16));
    }
    while usart2.sr.read().tc().bit_is_clear() 
    {
        // Transmitting data, bit_is_set will be true when transmission completes
    }
    });
}

fn handle_input<C, D>(t: &mut Threshold, input_command_position: &mut rtfm::Static<u8>, 
input_command: &mut rtfm::Static<[u8;100]>, usart2:  &mut C, received_char: u8, tim2: &mut D)
where
    C: Resource<Data = stm32f40x::USART2>,
    D: Resource<Data = stm32f40x::TIM2>,
{
    if received_char == '\r' as u8  // carriage return (Enter)
    {
        print_usart(t, usart2, "\n\r");     // New line and return to beginning of line
        execute_command(input_command_position, input_command, t, usart2, tim2);
        // reset input_command_position, could've reset the array but there is no need using this method.
        input_command_position.claim_mut(t, |input_command_position, _t| 
        {
            **input_command_position = 0;
        });
    }
    else 
    {
        // Add new character to the input_command array and increase input_command_position since input was not a \r character (Enter)
        let current_input_position = **input_command_position;
        input_command.claim_mut(t, |input_command, _t| {
            input_command[current_input_position as usize] = received_char;
        });
        input_command_position.claim_mut(t, |input_command_position, _t| {
            **input_command_position += 1;
        });
    }
}

fn execute_command<C, D>(input_command_position: &mut rtfm::Static<u8>, input_command: &mut rtfm::Static<[u8;100]>, 
t: &mut Threshold, usart2: &mut C, tim2: &mut D)
where
    C: Resource<Data = stm32f40x::USART2>,
    D: Resource<Data = stm32f40x::TIM2>,
{
    // Parse the input_command array to &str so matching becomes easier and we can perform splits
    let command = match core::str::from_utf8(&input_command[..**input_command_position as usize])
    {
        Ok(parsed_string) => 
        {
            parsed_string
        }
        Err(_) =>
        {
            print_usart(t, usart2, ">Can not parse input \n");
            return;
        }
    };
    let mut command_split = command.split(" ");
    match command_split.next().unwrap()
    {
        "pause" => 
        {
            tim2.claim_mut(t, |tim2, _t| 
            {
                tim2.dier.write(|w| w.uie().clear_bit());   // Clear timer interrupt
            });
            print_usart(t, usart2, ">Paused \n");
        },

        "start" => 
        {
            tim2.claim_mut(t, |tim2, _t| 
            {
                tim2.dier.write(|w| w.uie().set_bit());     // Set timer interrupt
            });
            print_usart(t, usart2, ">Started \n");
        },

        "period" =>
        {
            match command_split.next()
            {
                Some(value_given) =>
                {
                    let parsed_period : u32 = match value_given.parse()
                    {
                        Ok(number) => 
                        {
                            number
                        }
                        Err(_) =>
                        {
                            // period XXX given where XXX can not be parsed to a u32
                            print_usart(t, usart2, ">Unkown period given \n");
                            return;
                        }
                    };
                    // set the parsed_period as the new period in the TIM2 interrupt.
                    let new_interupt_value = (250000/1000) * parsed_period;
                    tim2.claim_mut(t, |tim2, _t| 
                    {
                        unsafe 
                        {
                            tim2.cnt.modify(|_, w| w.bits(0));
                            tim2.arr.write(|w| w.bits(new_interupt_value));
                        }
                    });
                    print_usart(t, usart2, ">Period updated with new period ");
                    print_usart(t, usart2, value_given);
                    print_usart(t, usart2, "\n");
                }
                None =>
                {
                    // command given "period" with no value given.
                    print_usart(t, usart2, ">No period value given \n");
                }
            }
        }
        // Something else then period, start and pause given as a command.
        unknown =>
        {
            print_usart(t, usart2, ">Unknown command : ");
            print_usart(t, usart2, unknown);
            print_usart(t, usart2, "\n");
        }
    }
}

fn print_usart<A>(t: &mut Threshold, usart2: &mut A,  message: &str)
where
    A: Resource<Data = stm32f40x::USART2>,
{
    // Go through the characters in the message and send them through USART connection.
    for character in message.chars() {
        usart2.claim_mut(t, |usart2, _t| {
            unsafe 
            {
                usart2.dr.write(|w| w.dr().bits(character as u16));
            }
            while usart2.sr.read().tc().bit_is_clear() 
            {
                
            }
        });
    }
}