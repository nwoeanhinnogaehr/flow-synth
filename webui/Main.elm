module Main exposing (..)

import Html exposing (Html, button, div, text, ol, ul, li, b, i, br, code, input)
import Html.Events exposing (onClick, onInput)
import Html.Attributes exposing (..)
import Json.Decode as Decode
import Json.Encode as Encode
import Http
import Task
import Array exposing (Array)
import WebSocket
import Dict exposing (Dict)


-- DATA --


type alias Model =
    { graph : Graph
    , err : Maybe String
    , mode : Mode
    , libPath : String
    , messageArgs : Dict String String
    , id : Int
    , apiUrl : String
    }


emptyModel =
    { graph = emptyGraph
    , err = Nothing
    , mode = Normal
    , libPath = ""
    , messageArgs = Dict.empty
    , id = 0
    , apiUrl = "http://localhost:8008/"
    }


type Mode
    = Normal
    | Connecting ConnectingState


type alias ConnectingState =
    { node : Node
    , port_ : Port
    , portType : PortType
    }


type alias Ports =
    { input : List Port
    , output : List Port
    }


type alias Port =
    { id : Int
    , edge : Maybe Edge
    , portType : PortType
    , name : String
    }


type PortType
    = Input
    | Output


type alias Edge =
    { nodeId : Int
    , portId : Int
    }


type alias Node =
    { id : Int
    , type_name : String
    , ports : Ports
    , messageDescriptors : List MessageDescriptor
    }


type alias MessageDescriptor =
    { id : Int
    , name : String
    , args : List MessageDescArg
    }


type alias MessageDescArg =
    { name : String
    , type_ : String
    }


type alias Message =
    { id : Int
    , args : List MessageArg
    }


type alias MessageArg =
    String


type alias NodeType =
    { name : String
    }


type alias Graph =
    { nodes : List Node
    , types : List NodeType
    , libs : List NodeLibrary
    }


type alias NodeLibrary =
    { name : String
    , path : String
    }


type Msg
    = ResponseWrapper (Result Http.Error Msg)
    | ErrorResponse Decode.Value
    | Refresh
    | Refreshed Graph
    | AddNode NodeType
    | AddedNode Int
    | StartConnecting Node Port
    | DoConnect Node Port Node Port
    | Connected Decode.Value
    | CancelConnect
    | DoDisconnect Node Port Edge
    | Disconnected Decode.Value
    | SendMessage Node Message
    | SentMessage Decode.Value
    | KillNode Node
    | KilledNode Decode.Value
    | ReloadNode Node
    | ReloadedNode Decode.Value
    | DataUpdate String
    | LoadLibrary String
    | LoadedLibrary Decode.Value
    | NewLibPath String
    | SetMessageArg String String
    | SetId String


emptyGraph =
    { nodes = []
    , types = []
    , libs = []
    }



-- VIEW --


connectView : Model -> Html Msg
connectView model =
    div []
        [ text "instance id:"
        , input [ placeholder "id", onInput SetId ] [ text "0" ]
        ]


libsView : Model -> Html Msg
libsView model =
    div []
        [ text "Node libraries:"
        , div []
            [ input [ placeholder "Path to new lib...", onInput NewLibPath ] []
            , button [ onClick (LoadLibrary model.libPath) ] [ text "Load" ]
            ]
        , ul [] (List.map (libView model) model.graph.libs)
        ]


libView : Model -> NodeLibrary -> Html Msg
libView model lib =
    li []
        [ text lib.name
        , button [ onClick (LoadLibrary lib.path) ] [ text "Reload" ]
        ]


typesView : Model -> Html Msg
typesView model =
    div []
        [ text "Node types:"
        , ol [] (List.map (typeView model) model.graph.types)
        ]


typeView : Model -> NodeType -> Html Msg
typeView model node =
    li [] [ button [ onClick (AddNode node) ] [ text node.name ] ]


nodesView : Model -> Html Msg
nodesView model =
    div []
        [ text "Nodes:"
        , ol [] (List.map (nodeView model) model.graph.nodes)
        ]


nodeView : Model -> Node -> Html Msg
nodeView model node =
    li []
        [ div []
            [ button [ onClick (KillNode node) ] [ text "Kill" ]
            , button [ onClick (ReloadNode node) ] [ text "Reload" ]
            , b [] [ text (node.type_name ++ " [" ++ toString node.id ++ "]") ]
            , ul [] (List.map (messageDescriptorView model node) node.messageDescriptors)
            , text "Inputs:"
            , ol [] (List.map (portView model node) node.ports.input)
            , text "Outputs:"
            , ol [] (List.map (portView model node) node.ports.output)
            ]
        ]


messageDescriptorView : Model -> Node -> MessageDescriptor -> Html Msg
messageDescriptorView model node desc =
    li []
        (List.map
            (\arg ->
                input [ placeholder arg.name, onInput (SetMessageArg (messageArgKey node desc arg)) ] []
            )
            desc.args
            ++ [ button [ onClick (SendMessage node { id = desc.id, args = (List.map (getMessageArg model node desc) desc.args) }) ] [ text desc.name ] ]
        )


getMessageArg model node desc arg =
    Maybe.withDefault "" (Dict.get (messageArgKey node desc arg) model.messageArgs)


messageArgKey node desc arg =
    toString node.id ++ "." ++ toString desc.id ++ "." ++ toString arg.name


portView : Model -> Node -> Port -> Html Msg
portView model node port_ =
    -- what a fucking mess
    li []
        [ div []
            (text port_.name
                :: (case model.mode of
                        Normal ->
                            [ Maybe.withDefault
                                (div []
                                    [ text "Disconnected. "
                                    , button [ onClick (StartConnecting node port_) ] [ text "Connect this..." ]
                                    ]
                                )
                                (Maybe.map
                                    (\edge ->
                                        div []
                                            [ (edgeView model edge)
                                            , button [ onClick (DoDisconnect node port_ edge) ] [ text "Disconnect this" ]
                                            ]
                                    )
                                    port_.edge
                                )
                            ]

                        Connecting state ->
                            if state.node == node && state.port_ == port_ then
                                [ button [ onClick CancelConnect ] [ text "Cancel" ]
                                ]
                            else if state.portType /= port_.portType then
                                [ (Maybe.withDefault
                                    (button
                                        [ onClick
                                            (if state.portType == Output then
                                                DoConnect state.node state.port_ node port_
                                             else
                                                DoConnect node port_ state.node state.port_
                                            )
                                        ]
                                        [ text "Connect here" ]
                                    )
                                    (Maybe.map (edgeView model) port_.edge)
                                  )
                                ]
                            else
                                [ (Maybe.withDefault
                                    (text "Disconnected.")
                                    (Maybe.map (edgeView model) port_.edge)
                                  )
                                ]
                   )
            )
        ]


edgeView : Model -> Edge -> Html Msg
edgeView model edge =
    b [] [ text (toString edge.nodeId ++ ":" ++ toString edge.portId) ]


errorView : Model -> Html Msg
errorView model =
    div [ style [ ( "color", "red" ) ] ] [ text (Maybe.withDefault "" model.err) ]



-- DECODE --


decodeGraph : Decode.Decoder Graph
decodeGraph =
    Decode.map3 Graph
        (Decode.field "nodes" decodeNodes)
        (Decode.field "types" decodeTypes)
        (Decode.field "libs" decodeLibs)


decodeLibs : Decode.Decoder (List NodeLibrary)
decodeLibs =
    Decode.list
        (Decode.map2 NodeLibrary
            (Decode.field "name" Decode.string)
            (Decode.field "path" Decode.string)
        )


decodeNodes : Decode.Decoder (List Node)
decodeNodes =
    Decode.list
        (Decode.map4 Node
            (Decode.field "id" Decode.int)
            (Decode.field "type_name" Decode.string)
            (Decode.field "ports" decodePorts)
            (Decode.field "message_descriptors" (Decode.list decodeMessageDescriptor))
        )


decodeMessageDescriptor : Decode.Decoder MessageDescriptor
decodeMessageDescriptor =
    Decode.map3 MessageDescriptor
        (Decode.field "id" Decode.int)
        (Decode.field "name" Decode.string)
        (Decode.field "args" (Decode.list decodeMessageDescArg))


decodeMessageDescArg : Decode.Decoder MessageDescArg
decodeMessageDescArg =
    -- TODO decode types to enum
    Decode.map2 MessageDescArg
        (Decode.field "name" Decode.string)
        (Decode.field "type" Decode.string)


decodePorts : Decode.Decoder Ports
decodePorts =
    Decode.map2 Ports
        (Decode.field "in" (Decode.list (decodePort Input)))
        (Decode.field "out" (Decode.list (decodePort Output)))


decodePort : PortType -> Decode.Decoder Port
decodePort portType =
    Decode.map4 Port
        (Decode.field "id" Decode.int)
        (Decode.maybe
            (Decode.map2 Edge
                (Decode.at [ "edge", "node" ] Decode.int)
                (Decode.at [ "edge", "port" ] Decode.int)
            )
        )
        (Decode.succeed portType)
        (Decode.field "name" Decode.string)


decodeTypes : Decode.Decoder (List NodeType)
decodeTypes =
    Decode.list
        (Decode.map NodeType
            (Decode.field "name" Decode.string)
        )


decodeAddNode : Decode.Decoder Int
decodeAddNode =
    Decode.field "id" Decode.int



-- WRAPPER --


decodeWrapper msg decoder =
    Decode.andThen
        (\status ->
            if status == "ok" then
                Decode.map msg
                    (Decode.field "data" decoder)
            else
                Decode.map ErrorResponse (Decode.field "data" Decode.value)
        )
        (Decode.field "status" Decode.string)


httpGet model path decoder msg =
    let
        url =
            model.apiUrl ++ path

        request =
            Http.get url (decodeWrapper msg decoder)
    in
        Http.send ResponseWrapper request


httpPost model path body decoder msg =
    let
        url =
            model.apiUrl ++ path

        request =
            Http.post url (Http.jsonBody body) (decodeWrapper msg decoder)
    in
        Http.send ResponseWrapper request



-- ADD NODE --


addNode : Model -> NodeType -> Cmd Msg
addNode model nodeType =
    httpGet
        model
        ("type/new/" ++ nodeType.name)
        decodeAddNode
        AddedNode



-- REFRESH --


doRefresh : Model -> Cmd Msg
doRefresh model =
    httpGet
        model
        "state"
        decodeGraph
        Refreshed


dataUpdate : Model -> String -> Model
dataUpdate model updateStr =
    let
        newGraph =
            Decode.decodeString decodeGraph updateStr
    in
        case newGraph of
            Ok newGraph ->
                { model | graph = newGraph }

            Err e ->
                raiseError model ("websocket parse: " ++ e)



-- CONNECT --


doConnect : Model -> Node -> Port -> Node -> Port -> Cmd Msg
doConnect model srcNode srcPort dstNode dstPort =
    httpGet
        model
        ("node/connect/"
            ++ toString srcNode.id
            ++ "/"
            ++ toString srcPort.id
            ++ "/to/"
            ++ toString dstNode.id
            ++ "/"
            ++ toString dstPort.id
        )
        decodeConnect
        Connected


decodeConnect =
    Decode.value


fullyConnected node =
    List.all portIsConnected node.ports.input
        && List.all portIsConnected node.ports.output


portIsConnected port_ =
    port_.edge /= Nothing


doDisconnect : Model -> Node -> Port -> Edge -> Cmd Msg
doDisconnect model node port_ edge =
    let
        ( endpointNode, endPointPort ) =
            case port_.portType of
                Input ->
                    ( node.id, port_.id )

                Output ->
                    ( edge.nodeId, edge.portId )

        url =
            "node/disconnect/"
                ++ toString endpointNode
                ++ "/"
                ++ toString endPointPort
    in
        httpGet model url decodeDisconnect Disconnected


decodeDisconnect =
    Decode.value



-- MESSAGES --


sendMessage model node msg =
    httpPost
        model
        ("node/send_message/" ++ toString node.id ++ "/" ++ toString msg.id)
        (Encode.list (List.map Encode.string msg.args))
        Decode.value
        SentMessage


killNode model node =
    httpGet
        model
        ("node/kill/" ++ toString node.id)
        Decode.value
        KilledNode


reloadNode model node =
    httpGet
        model
        ("node/reload/" ++ toString node.id)
        Decode.value
        ReloadedNode


loadLibrary model path =
    httpPost
        model
        "type/load_library/"
        (Encode.string path)
        Decode.value
        LoadedLibrary



-- ERROR --


raiseError : Model -> String -> Model
raiseError model err =
    { model | err = Just err }



-- SPECIAL FUNCTIONS --


setId model id =
    { model | id = id, apiUrl = "http://localhost:" ++ toString (8008 + id) ++ "/" }


init : ( Model, Cmd Msg )
init =
    ( emptyModel
    , doRefresh emptyModel
    )


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        ResponseWrapper (Ok nextMsg) ->
            ( model, Task.perform (\x -> x) (Task.succeed nextMsg) )

        ResponseWrapper (Err err) ->
            ( raiseError model (toString err), Cmd.none )

        ErrorResponse err ->
            ( raiseError model ("Server errored: " ++ toString err), Cmd.none )

        Refresh ->
            ( model, doRefresh model )

        Refreshed newGraph ->
            ( { model | graph = newGraph }, Cmd.none )

        AddNode nodeType ->
            ( model, addNode model nodeType )

        AddedNode nodeId ->
            ( model, doRefresh model )

        StartConnecting node port_ ->
            ( { model | mode = Connecting { node = node, port_ = port_, portType = port_.portType } }, Cmd.none )

        DoConnect srcNode srcPort dstNode dstPort ->
            ( { model | mode = Normal }, doConnect model srcNode srcPort dstNode dstPort )

        Connected value ->
            ( model, doRefresh model )

        CancelConnect ->
            ( { model | mode = Normal }, Cmd.none )

        DoDisconnect node port_ edge ->
            ( model, doDisconnect model node port_ edge )

        Disconnected value ->
            ( model, doRefresh model )

        SendMessage node message ->
            ( model, sendMessage model node message )

        SentMessage value ->
            ( model, doRefresh model )

        KillNode node ->
            ( model, killNode model node )

        KilledNode value ->
            ( model, doRefresh model )

        ReloadNode node ->
            ( model, reloadNode model node )

        ReloadedNode value ->
            ( model, doRefresh model )

        DataUpdate str ->
            ( dataUpdate model str, Cmd.none )

        LoadLibrary lib ->
            ( model, loadLibrary model lib )

        LoadedLibrary value ->
            ( model, doRefresh model )

        NewLibPath path ->
            ( { model | libPath = path }, Cmd.none )

        SetMessageArg key value ->
            ( { model | messageArgs = Dict.insert key value model.messageArgs }, Cmd.none )

        SetId id ->
            ( setId model (Result.withDefault 0 (String.toInt id)), Cmd.none )


subscriptions : Model -> Sub Msg
subscriptions model =
    WebSocket.listen ("ws://localhost:" ++ toString (3000 + model.id)) DataUpdate


view : Model -> Html Msg
view model =
    div []
        [ connectView model
        , errorView model
        , libsView model
        , typesView model
        , nodesView model
        ]


main =
    Html.program { init = init, view = view, update = update, subscriptions = subscriptions }
