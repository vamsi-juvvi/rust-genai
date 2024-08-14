# Automated prompt response agent

## TL;DR

 - original request `What's the weather like today in San Jose, CA` sent to `gpt-40-mini`
 - It's response analyzed by groq's `llama-3.1-8b-instant` to see whether it is a question asking to choose between celcius and fahrenheit and then responds with fahrenheit.
 - Great use of `genai` where different LLM providers/models used for different reasons
 - Added tracing so I could convert the print statements to `debug!` and `info!`

## Details

This continues from [1-ExploreAndProtoypeToolCalls](./1-ExploreAndProtoypeToolCalls.md)

`gpt-4o-mini` follows the prompt up with **What temperature unit would you like the weather in? Celsius or Fahrenheit?**. Hard to believe that the model got this wrong. Maybe the system prompt of **"Don't make assumptions about what values to plug into functions. Ask for clarification if a user request is ambiguous."** made it more conservative.

# Basic prompt-wrangling agent

I have two options here.

 - change prompt to **"What's the weather like today in San Jose, CA? Respond in Celcius units."**. This worked. The tool_call came in with `"San Jose, CA", "Celcius"`

Since I have the time, I decided to get creative. 
 - ⭐ **How about, I call an LLM to figure out if the LLM's question is asking me to decide on a temp unit?**
 - This is pretty much baby-steps in agenting. 


> This is a common approach to take though. When LLM responses can keep varying, best to ask the LLM itself about it's response. This is done for unit tests for instance. The decision here is whether you can use a cheaper/faster LLM to classify the primary LLM response: an ideal use for genai. Targeting the much cheaper/faster groq for these types of questions seems ideal. Even if cost is not a concern, the high output token throughput of groq may drive the decision.

After some trial and error, I came up with the following setup


## Respond to LLM followup question with an LLM classification

Check the first LLM response and respond with "Celcius" if appropriate. Use the **Is this a question about temperature units?** prompt to evaluate/classify the **Would you like the temperature in Celcius or Farenheit?** clarification from the LLM.

```rust
debug!("{:<12} -  {}", "c06 - processing payload", resp);

let yn = llm_is_yes_no(&client, resp.to_string(), 
    "Is this a question about temperature units?".to_string())
    .await?;

if yn {
    // Randomly choose celsius as the response
    debug!("{:<12} -  {}", "c06 - Responding with Celcius to", resp);                    
    followup_msgs
    .get_or_insert(Vec::new())
    .push(
        ChatMessage::user("celsius")
    );
}
```

 - The prompt **Is this a question about temperature units?**
 - Call to `llm_is_yes_no` to use the prompt

## Build the `llm_is_yes_no` helper function

`llm_is_yes_no` is modeled very simply. The chaining of the `Result` and `Option`, being a rust newbie took some work for me, but in the end, I prevailed. I had also decided to use the faster `llama-3.1-8b-instant` model from groq for this question. I'd imagine that in production uses, one would use a small self-hosted and maybe quantized model for these smaller NLU tasks. 

This is what I built (_goes to show how usable J. Chone's genai lib is 👏_). 

```rust
const INSTANT_MODEL: &str = "llama-3.1-8b-instant";

// question: A prompt question which'd expect a yes/no response.
// context: The document on which the question is being asked.
async fn llm_is_yes_no(client: &Client, context: String, question: String) -> Result<bool> {
    debug!("{:<12} - Calling groq with question :{} on context: {}", "c06 - llm_is_yes_no", question, context);

    let chat_req = ChatRequest::default()
        .with_system("You are an assistant that answers questions with a yes or no.")
        .append_message(
            ChatMessage::user(format!(
                "I have a question about the text enclosed in triple quotes.
                Please answer it with either a yes or no with no punctuation.

                question : {}
                '''
                {}
                '''", question, context)
        ));

    let chat_res = client
        .exec_chat(INSTANT_MODEL, chat_req, None)
        .await?;

    if let ChatResponsePayload::Content(opt_mc) = chat_res.payload 
    {             
        opt_mc
            .as_ref()
            .and_then(MessageContent::text_as_str)
            .map( |v| {
                    debug!("{:<12} - Processing response {:?}", "c06 - llm_is_yes_no", v);                    
                    match v.trim().to_lowercase().as_str() {
                        "yes" => Ok(true),
                        "no" => Ok(false),
                        _ => {                    
                            Err(Error::BotResponseIsUnexpected {
                                expected : "\"yes\" or \"no\"".to_string(),
                                got : v.to_string()
                            })
                        },
                    }
                })
                .unwrap_or(Err(Error::BotResponseIsEmpty))
    }
    else 
    {        
    Err(Error::BotResponseIsUnexpected{
        expected: "MessageContext response".to_string(),
        got: "Something else".to_string()})
    }

} 
```

I decided to use a simple `match { "yes" => .., "no" =>}` to process the response to the query. However, it gave me a `Yes.` So added the `with no punctuation` bit to the prompt.

## Build an agent response loop

Now that I can use another LLM to check the first LLM's _Would you like the temp in celcius or fahrenheit_ question. Time to automate this step in a loop. The loop can start out real simple:

 - Process `assistant` response
   - Check if it need the `user` to followup 
   - If Yes: Add the followup `rol=user` `ChatMessage`
 - End loop if there are no more followup `ChatMessages`


```diff
+loop {
        let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;

+        // Holds followup chatMessages to send, continues loop.
+        let mut followup_msgs:Option<Vec<ChatMessage>> = None;
        
 --snip--

+        // Continue chat as long as we have followup messages
+        if let Some(msgs) = followup_msgs {
+            for msg in msgs {
+                chat_req = chat_req.append_message(msg);
+            }            
+        } else {
+            break;
+        }
    }
```

## Check bot response and followup

This portion made use of code I had already built up.

```diff
loop {
        let chat_res = client.exec_chat(MODEL, chat_req.clone(), None).await?;
        
        let mut followup_msgs:Option<Vec<ChatMessage>> = None;

        // This crude way of detecting followup questions about which temp
        // sometimes backfires. It needs a stack where once tool-call has been 
        // responded-to. We no longer expect it to ask clarifying questions about
        // which temp unit to use.        

        match chat_res.payload {
            ChatResponsePayload::Content(opt_mc) => {
                let resp = opt_mc
                .as_ref()
                .and_then(MessageContent::text_as_str)
                .unwrap_or("NO ANSWER");

+                debug!("{:<12} -  {}", "c06 - processing payload", resp);
+
+                let yn = llm_is_yes_no(&client, resp.to_string(), 
+                    "Is this a question about temperature units?".to_string())                    
+                    .await?;
+
+                if yn {
+                    // Randomly choose celsius as the response
+                    debug!("{:<12} - Responding with Celcius to {}", "c06", resp);                    
+                    followup_msgs
+                    .get_or_insert(Vec::new())
+                    .push(
+                        ChatMessage::user("celsius")
+                    );
+                }
            },
            ChatResponsePayload::ToolCall(opt_tc) => {            
                if let Some(tc_vec) =  opt_tc {                    
                    debug!("{:<12} -  {:?}", "c06 - Responding to tool_calls", &tc_vec);
                    
                    let vec = followup_msgs.get_or_insert(Vec::new());

                    // OpenAI requires that the assistant's tool_call request be added back 
                    // to the chat. Without this, it will reject the subsequent "role=tool" msg.
                    vec.push(tc_vec.clone().into());

                    for tool_call in &tc_vec {

                        debug!("{:<12} - Handling tool_call req for {}", "c06", tool_call.function.fn_name);

                        if tool_call.function.fn_name == "get_current_weather" {
                            // Fake it for now. Simply return 75F
                            let tool_response_msg = ChatMessage::tool(
                                tool_call.tool_call_id.clone(), 
                                tool_call.function.fn_name.clone(),
                                "75F".to_string());
                            
                            debug!("{:<12} - Adding tool_call response {}", "c06", &tool_response_msg);                            

                            vec.push(tool_response_msg);
                        }
                    }                    
                }
            }
        }    

        // Continue chat as long as we have followup messages
        if let Some(msgs) = followup_msgs {
            for msg in msgs {
                chat_req = chat_req.append_message(msg);
            }            
        } else {
            break;
        }
    }
	
```

👏 Yay! this worked great. Simplistic but worked.

Worked for all of one day. On `8/1/2024` it broke. The loop just wouldn't terminate 😱. Imagine the bill you'd get if you were calling OpenAI in a fast loop! _Wonder how production use cases implement short circuts on API calls?_.

Turns out, `gpt-4o-mini` decided to get more helpeful overnight. The final response from it changed from 
 - _The current weather in San Jose, CA is 75°F_
 - **to**
 - _The current weather in San Jose, CA is 75°F. Would you like the temperature converted to Celsius?_
 - The **Would you like temp converted to Celcius** tripped my generic question: **Is this a question about temperature units** and the loop kep continuing. 

> Critical lesson here. Any loop better have guardrails, circuit breakers and way better prompt engineering. Also conceptually to check if it asking specific questions regarding tool arguments and once that tool has been executed, don't expect any more such questions. 
>
> Maybe also get a human in the loop for critical stuff?

## Refine my prompt

How about I change the prompt from _Is this a question about temperature units_ to _Is this a question asking to choose between Celcius and Fahrtenheit?_ 

This worked.

## Final sequence of API calls and responses in debug traces

```bash
DEBUG c06 - get_current_weather tool schema -  {
  "function": {
    "description": "Get the current weather",
    "name": "get_current_weather",
    "parameters": {
      "properties": {
        "format": {
          "description": "The temperature unit to use. Infer this from the users location.",
          "enum": [
            "Celcius",
            "Farenheit"
          ],
          "type": "string"
        },
        "location": {
          "description": "The city and state, e.g. San Francisco, CA",
          "type": "string"
        }
      },
      "required": [
        "format",
        "location"
      ],
      "type": "object"
    }
  },
  "type": "function"
}
DEBUG starting new connection: https://api.openai.com/    
DEBUG resolving host="api.openai.com"
DEBUG connecting to 104.18.6.192:443
DEBUG connected to 104.18.6.192:443
DEBUG pooling idle connection for ("https", api.openai.com)
DEBUG gpt-4o-mini - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "Would you like the temperature in Celsius or Fahrenheit?",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542261,
  "id": "chatcmpl-9rWDdKZkE8h71gM7sZqirDvFA3Uaq",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_0f03d4f0ee",
  "usage": {
    "completion_tokens": 11,
    "prompt_tokens": 189,
    "total_tokens": 200
  }
}
DEBUG c06 - processing payload -  Would you like the temperature in Celsius or Fahrenheit?
DEBUG c06 - llm_is_yes_no - Calling groq with question :Is this a question asking to choose between celcius and fahrenheit? on context: Would you like the temperature in Celsius or Fahrenheit?
DEBUG starting new connection: https://api.groq.com/    
DEBUG resolving host="api.groq.com"
DEBUG connecting to 104.18.2.205:443
DEBUG connected to 104.18.2.205:443
DEBUG pooling idle connection for ("https", api.groq.com)
DEBUG llama-3.1-8b-instant - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "Yes",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542266,
  "id": "chatcmpl-c14c6bdc-f931-4732-8275-a0800042606e",
  "model": "llama-3.1-8b-instant",
  "object": "chat.completion",
  "system_fingerprint": "fp_9cb648b966",
  "usage": {
    "completion_time": 0.002666667,
    "completion_tokens": 2,
    "prompt_time": 0.026192122,
    "prompt_tokens": 87,
    "total_time": 0.028858789000000003,
    "total_tokens": 89
  },
  "x_groq": {
    "id": "req_01j47ra22fe9vsege100b2y6dh"
  }
}
DEBUG c06 - llm_is_yes_no - Processing response "Yes"
DEBUG c06          - Responding with Celcius to Would you like the temperature in Celsius or Fahrenheit?
DEBUG reuse idle connection for ("https", api.openai.com)
DEBUG pooling idle connection for ("https", api.openai.com)
DEBUG gpt-4o-mini - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "tool_calls",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": null,
        "role": "assistant",
        "tool_calls": [
          {
            "function": {
              "arguments": "{\"format\":\"Celcius\",\"location\":\"San Jose, CA\"}",
              "name": "get_current_weather"
            },
            "id": "call_wDYl8wkSOc6FmxqsfUIxHc2f",
            "type": "function"
          }
        ]
      }
    }
  ],
  "created": 1722542267,
  "id": "chatcmpl-9rWDj2ro6K5o3golrR8NNGKLV9jao",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_0f03d4f0ee",
  "usage": {
    "completion_tokens": 23,
    "prompt_tokens": 195,
    "total_tokens": 218
  }
}
DEBUG OpenAI.to_web_request_data/tool_calls -  [
  {
    "function": {
      "arguments": "{\"format\":\"Celcius\",\"location\":\"San Jose, CA\"}",
      "name": "get_current_weather"
    },
    "id": "call_wDYl8wkSOc6FmxqsfUIxHc2f",
    "type": "function"
  }
]
DEBUG c06 - Responding to tool_calls -  [AssistantToolCall { tool_call_id: "call_wDYl8wkSOc6FmxqsfUIxHc2f", tool_call_type: "function", function: AssistantToolCallFunction { fn_name: "get_current_weather", fn_arguments: Some(Object {"format": String("Celcius"), "location": String("San Jose, CA")}) } }]
DEBUG c06          - Handling tool_call req for get_current_weather
DEBUG c06          - Adding tool_call response Tool(ToolMessage { tool_call_id: "call_wDYl8wkSOc6FmxqsfUIxHc2f", tool_name: "get_current_weather", tool_result: "75F" })
DEBUG reuse idle connection for ("https", api.openai.com)
DEBUG pooling idle connection for ("https", api.openai.com)
DEBUG gpt-4o-mini - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "The current temperature in San Jose, CA is 75°F, which is approximately 24°C.",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542268,
  "id": "chatcmpl-9rWDkvX4Yrgw4PIqZLIhweFu7dv0n",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_0f03d4f0ee",
  "usage": {
    "completion_tokens": 21,
    "prompt_tokens": 229,
    "total_tokens": 250
  }
}
DEBUG c06 - processing payload -  The current temperature in San Jose, CA is 75°F, which is approximately 24°C.
DEBUG c06 - llm_is_yes_no - Calling groq with question :Is this a question asking to choose between celcius and fahrenheit? on context: The current temperature in San Jose, CA is 75°F, which is approximately 24°C.
DEBUG reuse idle connection for ("https", api.groq.com)
DEBUG pooling idle connection for ("https", api.groq.com)
DEBUG llama-3.1-8b-instant - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "yes",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542272,
  "id": "chatcmpl-ddbf1f06-6f3b-4bc5-9340-ca0d5d93e633",
  "model": "llama-3.1-8b-instant",
  "object": "chat.completion",
  "system_fingerprint": "fp_9cb648b966",
  "usage": {
    "completion_time": 0.002666667,
    "completion_tokens": 2,
    "prompt_time": 0.028914806,
    "prompt_tokens": 97,
    "total_time": 0.031581473,
    "total_tokens": 99
  },
  "x_groq": {
    "id": "req_01j47ra88rfqtra8y3s9q1p3ba"
  }
}
DEBUG c06 - llm_is_yes_no - Processing response "yes"
DEBUG c06          - Responding with Celcius to The current temperature in San Jose, CA is 75°F, which is approximately 24°C.
DEBUG reuse idle connection for ("https", api.openai.com)
DEBUG pooling idle connection for ("https", api.openai.com)
DEBUG gpt-4o-mini - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "The current weather in San Jose, CA is 24 degrees Celsius.",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542272,
  "id": "chatcmpl-9rWDoSLaH3VQwiLmeYBaO4RQ2nifN",
  "model": "gpt-4o-mini-2024-07-18",
  "object": "chat.completion",
  "system_fingerprint": "fp_0f03d4f0ee",
  "usage": {
    "completion_tokens": 15,
    "prompt_tokens": 235,
    "total_tokens": 250
  }
}
DEBUG c06 - processing payload -  The current weather in San Jose, CA is 24 degrees Celsius.
DEBUG c06 - llm_is_yes_no - Calling groq with question :Is this a question asking to choose between celcius and fahrenheit? on context: The current weather in San Jose, CA is 24 degrees Celsius.
DEBUG reuse idle connection for ("https", api.groq.com)
DEBUG pooling idle connection for ("https", api.groq.com)
DEBUG llama-3.1-8b-instant - OpenAI.to_web_request_data {
  "choices": [
    {
      "finish_reason": "stop",
      "index": 0,
      "logprobs": null,
      "message": {
        "content": "no",
        "role": "assistant"
      }
    }
  ],
  "created": 1722542277,
  "id": "chatcmpl-437eb8ed-2699-4ad6-82e9-ba88df5c845c",
  "model": "llama-3.1-8b-instant",
  "object": "chat.completion",
  "system_fingerprint": "fp_9cb648b966",
  "usage": {
    "completion_time": 0.002666667,
    "completion_tokens": 2,
    "prompt_time": 0.027316356,
    "prompt_tokens": 91,
    "total_time": 0.029983023,
    "total_tokens": 93
  },
  "x_groq": {
    "id": "req_01j47rachdfqvsw4h8bj7z17ts"
  }
}
DEBUG c06 - llm_is_yes_no - Processing response "no"
```

